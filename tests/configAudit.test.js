"use strict";

const express = require("express");
const request = require("supertest");
const { createConfigAuditRouter } = require("../src/routes/configAudit");
const { RuntimeConfigAuditor, fingerprint, loadRuntimeConfig, redactConfig } = require("../src/services/configAudit");

describe("RuntimeConfigAuditor", () => {
  it("produces stable fingerprints independent of key order", () => {
    expect(fingerprint({ b: 2, a: 1 })).toBe(fingerprint({ a: 1, b: 2 }));
  });

  it("redacts sensitive config keys before exposing snapshots", () => {
    expect(redactConfig({ api_key: "secret", port: 3000 })).toEqual({ api_key: "[REDACTED]", port: 3000 });
  });

  it("detects missing, unexpected, and changed runtime configuration", () => {
    const auditor = new RuntimeConfigAuditor({
      baseline: { feature: true, port: 3000 },
      now: () => new Date("2026-01-01T00:00:00.000Z"),
    });

    const event = auditor.audit({ feature: false, extra: "value" }, { service: "unit" });

    expect(event.drift_detected).toBe(true);
    expect(event.drift).toEqual([
      { key: "extra", type: "unexpected", actual: "value" },
      { key: "feature", type: "changed", expected: true, actual: false },
      { key: "port", type: "missing", expected: 3000 },
    ]);
    expect(auditor.metrics().config_drift_active).toBe(1);
  });

  it("loads the bounded runtime configuration contract from environment", () => {
    expect(loadRuntimeConfig({ PORT: "8080", RATE_LIMIT_CAPACITY: "5" })).toMatchObject({
      port: 8080,
      rate_limit_capacity: 5,
      node_env: "development",
    });
  });
});

describe("config audit route", () => {
  it("returns 409 when runtime configuration drifts from baseline", async () => {
    const app = express();
    const auditor = new RuntimeConfigAuditor({ baseline: { port: 9999 } });
    app.use("/ops/config", createConfigAuditRouter(auditor));

    const res = await request(app).get("/ops/config/snapshot").expect(409);

    expect(res.body.drift_detected).toBe(true);
    expect(res.body.baseline_hash).toBe(auditor.baselineHash);
  });
});
