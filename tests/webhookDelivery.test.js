"use strict";

const request = require("supertest");
const app = require("../src/index");
const {
  buildSignatureHeaders,
  computeSignature,
  deliverWebhook,
  nextDelayMs,
  resetMetrics,
  snapshotMetrics,
  verifySignature,
} = require("../src/services/webhookDelivery");

const endpoint = { url: "https://partner.example/webhook", secret: "super-secret-value" };
const event = { id: "evt_123", type: "escrow.released", created_at: "2026-07-17T00:00:00.000Z", data: { escrow_id: "escrow-1", amount: "100" } };

describe("webhookDelivery service", () => {
  beforeEach(resetMetrics);

  it("signs payloads deterministically and verifies valid signatures", () => {
    const payload = { b: 2, a: 1 };
    const timestamp = 1784246400;
    const signature = computeSignature({ secret: endpoint.secret, timestamp, payload });
    expect(signature).toMatch(/^[a-f0-9]{64}$/);
    expect(verifySignature({ secret: endpoint.secret, timestamp, payload, signature: `sha256=${signature}`, now: timestamp })).toBe(true);
  });

  it("rejects invalid signatures and increments verification metrics", () => {
    expect(verifySignature({ secret: endpoint.secret, timestamp: 1784246400, payload: { a: 1 }, signature: "sha256=00", now: 1784246400 })).toBe(false);
    expect(snapshotMetrics().signatureVerificationFailed).toBe(1);
  });

  it("rejects stale timestamps to limit replay attacks", () => {
    const headers = buildSignatureHeaders({ secret: endpoint.secret, payload: event.data, timestamp: 1000 });
    expect(verifySignature({ secret: endpoint.secret, payload: event.data, timestamp: headers["x-agritrust-webhook-timestamp"], signature: headers["x-agritrust-webhook-signature"], now: 2000 })).toBe(false);
  });

  it("delivers successfully on the first 2xx response", async () => {
    const fetchImpl = jest.fn().mockResolvedValue({ status: 204 });
    await expect(deliverWebhook(endpoint, event, { fetchImpl, disableDelay: true })).resolves.toEqual({ ok: true, attempts: 1 });
    expect(fetchImpl).toHaveBeenCalledTimes(1);
    expect(fetchImpl.mock.calls[0][1].headers["x-agritrust-webhook-signature"]).toMatch(/^sha256=/);
    expect(snapshotMetrics()).toMatchObject({ enqueued: 1, delivered: 1, failed: 0, attempts: 1 });
  });

  it("retries non-2xx responses with exponential backoff", async () => {
    const fetchImpl = jest.fn()
      .mockResolvedValueOnce({ status: 500 })
      .mockResolvedValueOnce({ status: 503 })
      .mockResolvedValueOnce({ status: 200 });
    await expect(deliverWebhook(endpoint, event, { fetchImpl, disableDelay: true, maxAttempts: 3 })).resolves.toEqual({ ok: true, attempts: 3 });
    expect(fetchImpl).toHaveBeenCalledTimes(3);
    expect(nextDelayMs(1, { initialDelayMs: 100, backoffFactor: 2, maxDelayMs: 5000 })).toBe(100);
    expect(nextDelayMs(3, { initialDelayMs: 100, backoffFactor: 2, maxDelayMs: 5000 })).toBe(400);
  });

  it("returns failure after exhausting retry attempts", async () => {
    const fetchImpl = jest.fn().mockRejectedValue(new Error("network down"));
    await expect(deliverWebhook(endpoint, event, { fetchImpl, disableDelay: true, maxAttempts: 2 })).resolves.toEqual({ ok: false, attempts: 2 });
    expect(snapshotMetrics()).toMatchObject({ enqueued: 1, delivered: 0, failed: 1, attempts: 2 });
  });

  it("rejects weak endpoint secrets", async () => {
    await expect(deliverWebhook({ url: "https://partner.example/webhook", secret: "short" }, event, { fetchImpl: jest.fn() })).rejects.toMatchObject({ statusCode: 400 });
  });
});

describe("webhook routes", () => {
  beforeEach(() => {
    resetMetrics();
    process.env.WEBHOOK_VERIFICATION_SECRET = endpoint.secret;
  });

  afterEach(() => delete process.env.WEBHOOK_VERIFICATION_SECRET);

  it("exposes webhook delivery metrics", async () => {
    const res = await request(app).get("/webhooks/metrics");
    expect(res.status).toBe(200);
    expect(res.body).toHaveProperty("p99DeliveryLatencyMs");
  });

  it("verifies signed inbound webhook payloads", async () => {
    const payload = { status: "ok" };
    const headers = buildSignatureHeaders({ secret: endpoint.secret, payload });
    const res = await request(app).post("/webhooks/verify").set(headers).send(payload);
    expect(res.status).toBe(200);
    expect(res.body.verified).toBe(true);
  });

  it("rejects unsigned inbound webhook payloads", async () => {
    const res = await request(app).post("/webhooks/verify").send({ status: "ok" });
    expect(res.status).toBe(401);
    expect(res.body.verified).toBe(false);
  });
});
