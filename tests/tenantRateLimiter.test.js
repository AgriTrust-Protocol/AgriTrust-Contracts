"use strict";

const express = require("express");
const request = require("supertest");
const {
  TokenBucket,
  createTenantRateLimiter,
  normaliseTenantId,
} = require("../src/middleware/tenantRateLimiter");

function makeApp(limiter) {
  const app = express();
  app.use(limiter);
  app.get("/health", (_req, res) => res.status(200).json({ ok: true }));
  return app;
}

describe("TokenBucket", () => {
  it("allows requests while tokens remain and rejects when exhausted", () => {
    let nowMs = 0;
    const bucket = new TokenBucket({ capacity: 2, refillPerSecond: 1, now: () => nowMs });

    expect(bucket.consume(1, nowMs).allowed).toBe(true);
    expect(bucket.consume(1, nowMs).allowed).toBe(true);
    const denied = bucket.consume(1, nowMs);

    expect(denied.allowed).toBe(false);
    expect(denied.retryAfterSeconds).toBe(1);
  });

  it("refills tokens over elapsed time without exceeding capacity", () => {
    let nowMs = 0;
    const bucket = new TokenBucket({ capacity: 3, refillPerSecond: 2, now: () => nowMs });

    expect(bucket.consume(3, nowMs).allowed).toBe(true);
    nowMs = 500;
    expect(bucket.consume(1, nowMs).allowed).toBe(true);
    nowMs = 10_000;

    expect(bucket.snapshot(nowMs).remaining).toBe(3);
  });
});

describe("normaliseTenantId", () => {
  it("defaults missing tenant id to anonymous", () => {
    expect(normaliseTenantId(undefined)).toBe("anonymous");
    expect(normaliseTenantId("   ")).toBe("anonymous");
  });

  it("rejects malformed tenant ids", () => {
    expect(() => normaliseTenantId("tenant<script>")).toThrow("Invalid tenant id");
    expect(() => normaliseTenantId("a".repeat(129))).toThrow("Invalid tenant id");
  });
});

describe("createTenantRateLimiter middleware", () => {
  it("tracks token buckets independently per tenant", async () => {
    let nowMs = 0;
    const limiter = createTenantRateLimiter({ capacity: 1, refillPerSecond: 1, now: () => nowMs });
    const app = makeApp(limiter);

    expect((await request(app).get("/health").set("x-tenant-id", "tenant-a")).status).toBe(200);
    expect((await request(app).get("/health").set("x-tenant-id", "tenant-a")).status).toBe(429);
    expect((await request(app).get("/health").set("x-tenant-id", "tenant-b")).status).toBe(200);
  });

  it("sets rate limit and retry headers", async () => {
    const limiter = createTenantRateLimiter({ capacity: 1, refillPerSecond: 1, now: () => 0 });
    const app = makeApp(limiter);

    await request(app).get("/health").set("x-tenant-id", "tenant-a").expect(200);
    const res = await request(app).get("/health").set("x-tenant-id", "tenant-a").expect(429);

    expect(res.headers["x-ratelimit-limit"]).toBe("1");
    expect(res.headers["x-ratelimit-remaining"]).toBe("0");
    expect(res.headers["x-ratelimit-tenant"]).toBe("tenant-a");
    expect(res.headers["retry-after"]).toBe("1");
    expect(res.body.error).toBe("Rate limit exceeded");
  });

  it("refills the tenant bucket after enough time elapses", async () => {
    let nowMs = 0;
    const limiter = createTenantRateLimiter({ capacity: 1, refillPerSecond: 1, now: () => nowMs });
    const app = makeApp(limiter);

    await request(app).get("/health").set("x-tenant-id", "tenant-a").expect(200);
    await request(app).get("/health").set("x-tenant-id", "tenant-a").expect(429);
    nowMs = 1000;
    await request(app).get("/health").set("x-tenant-id", "tenant-a").expect(200);
  });

  it("rejects invalid tenant ids before route handlers", async () => {
    const limiter = createTenantRateLimiter({ capacity: 10, refillPerSecond: 10 });
    const app = makeApp(limiter);

    const res = await request(app).get("/health").set("x-tenant-id", "bad tenant!").expect(400);
    expect(res.body.error).toBe("Invalid tenant id");
  });
});
