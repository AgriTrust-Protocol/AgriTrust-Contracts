"use strict";

const { AdaptivePostgresPool } = require("../src/services/postgresPoolHealth");
const { createHealthRouter } = require("../src/routes/health");
const express = require("express");
const request = require("supertest");

function makePoolClass({ queryFails = false, totalCount = 1, waitingCount = 0 } = {}) {
  return class FakePool {
    constructor(opts) {
      this.opts = opts;
      this.totalCount = totalCount;
      this.idleCount = 0;
      this.waitingCount = waitingCount;
      FakePool.instances.push(this);
    }
    async connect() {
      return {
        query: jest.fn(async () => {
          if (queryFails) throw new Error("db down");
          return { rows: [{ "?column?": 1 }] };
        }),
        release: jest.fn(),
      };
    }
    async end() {}
  };
}

describe("AdaptivePostgresPool", () => {
  it("reports a healthy probe below the critical-path latency target", async () => {
    const Pool = makePoolClass();
    Pool.instances = [];
    const health = new AdaptivePostgresPool({ Pool, config: { max: 10, floor: 2, ceiling: 20 } });

    const result = await health.probe();

    expect(result.ok).toBe(true);
    expect(result.latency_ms).toBeLessThan(100);
    expect(result.pool.max).toBeGreaterThanOrEqual(2);
  });

  it("safe-fails closed when the health query fails", async () => {
    const Pool = makePoolClass({ queryFails: true });
    Pool.instances = [];
    const health = new AdaptivePostgresPool({ Pool });

    const result = await health.probe();

    expect(result.ok).toBe(false);
    expect(result.status).toBe("degraded");
    expect(health.metrics.probe_failures_total).toBe(1);
  });

  it("adapts pool size upward under saturation", async () => {
    const Pool = makePoolClass({ totalCount: 9, waitingCount: 2 });
    Pool.instances = [];
    const health = new AdaptivePostgresPool({ Pool, config: { max: 10, ceiling: 14, scaleStep: 2 } });

    await health.probe();

    expect(health.currentMax).toBe(12);
    expect(health.metrics.resizes_total).toBe(1);
  });

  it("returns disabled when no PostgreSQL pool is configured", async () => {
    const health = new AdaptivePostgresPool();
    await expect(health.probe()).resolves.toMatchObject({ ok: false, status: "disabled" });
  });
});

describe("health routes", () => {
  it("exposes /health/postgres and /health/metrics", async () => {
    const fakeHealth = {
      probe: jest.fn(async () => ({ ok: true, status: "ok", latency_ms: 3, pool: { max: 10 } })),
      prometheusMetrics: jest.fn(() => "agritrust_postgres_pool_max 10"),
    };
    const app = express();
    app.use("/health", createHealthRouter(fakeHealth));

    const probe = await request(app).get("/health/postgres");
    expect(probe.status).toBe(200);
    expect(probe.body.status).toBe("ok");

    const metrics = await request(app).get("/health/metrics");
    expect(metrics.status).toBe(200);
    expect(metrics.text).toContain("agritrust_postgres_pool_max 10");
  });
});
