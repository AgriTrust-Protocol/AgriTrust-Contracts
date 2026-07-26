"use strict";

const fs = require("fs");
const os = require("os");
const path = require("path");
const request = require("supertest");
const { ConfigManager, resetConfigMetrics, validateConfig } = require("../src/services/configManager");

function tempConfig(body) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "agritrust-config-"));
  const file = path.join(dir, "runtime.json");
  fs.writeFileSync(file, JSON.stringify(body), "utf8");
  return file;
}

describe("validateConfig", () => {
  it("applies defaults and validates the runtime schema", () => {
    const config = validateConfig({ rateLimit: { capacity: 10 } });
    expect(config.rateLimit.capacity).toBe(10);
    expect(config.rateLimit.refillPerSecond).toBe(30);
    expect(config.features.escrowRead).toBe(true);
  });

  it("rejects invalid values before they become active", () => {
    expect(() => validateConfig({ rateLimit: { capacity: 0 } })).toThrow("rateLimit.capacity");
    expect(() => validateConfig({ features: { escrowRead: "yes" } })).toThrow("features.escrowRead");
  });
});

describe("ConfigManager", () => {
  beforeEach(resetConfigMetrics);

  it("hot-reloads valid config while retaining last known-good config on validation failure", () => {
    const file = tempConfig({ rateLimit: { capacity: 2, refillPerSecond: 1 } });
    const manager = new ConfigManager({ configPath: file });

    expect(manager.reload().ok).toBe(true);
    expect(manager.current().rateLimit.capacity).toBe(2);

    fs.writeFileSync(file, JSON.stringify({ rateLimit: { capacity: 5, refillPerSecond: 2 } }), "utf8");
    expect(manager.reload().ok).toBe(true);
    expect(manager.current().rateLimit.capacity).toBe(5);

    fs.writeFileSync(file, JSON.stringify({ rateLimit: { capacity: -1 } }), "utf8");
    const failed = manager.reload();
    expect(failed.ok).toBe(false);
    expect(manager.current().rateLimit.capacity).toBe(5);
    expect(manager.metrics().validationFailures).toBe(1);
  });

  it("exposes runtime config and manual reload endpoints", async () => {
    const file = tempConfig({ rateLimit: { capacity: 7, refillPerSecond: 3 } });
    process.env.AGRITRUST_CONFIG_PATH = file;
    jest.resetModules();
    const app = require("../src/index");

    let res = await request(app).get("/ops/config/runtime").set("x-tenant-id", "ops");
    expect(res.status).toBe(200);
    expect(res.body.config.rateLimit.capacity).toBe(7);

    fs.writeFileSync(file, JSON.stringify({ rateLimit: { capacity: 9, refillPerSecond: 3 } }), "utf8");
    res = await request(app).post("/ops/config/reload").set("x-tenant-id", "ops");
    expect(res.status).toBe(202);
    expect(res.body.config.rateLimit.capacity).toBe(9);

    app.configManager.stop();
    delete process.env.AGRITRUST_CONFIG_PATH;
  });
});
