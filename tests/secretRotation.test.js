"use strict";

const request = require("supertest");
const app = require("../src/index");
const {
  InMemorySecretProvider,
  SecretRotationService,
  SECRET_TYPES,
  fingerprint,
  generateSecretValue,
} = require("../src/services/secretRotation");

describe("SecretRotationService", () => {
  const fixedNow = Date.parse("2026-07-17T00:00:00.000Z");

  function buildService(rotatedAt = "2026-06-01T00:00:00.000Z") {
    const provider = new InMemorySecretProvider([
      {
        name: "database.primary",
        type: SECRET_TYPES.DATABASE,
        value: "postgres://old",
        version: 7,
        rotatedAt,
      },
    ]);
    return {
      provider,
      service: new SecretRotationService({
        provider,
        clock: () => fixedNow,
        logger: { info: jest.fn(), warn: jest.fn() },
      }),
    };
  }

  it("rotates database credentials without returning secret material", async () => {
    const { provider, service } = buildService();

    const result = await service.rotateSecret("database.primary");
    const stored = await provider.getSecret("database.primary");

    expect(result.value).toBeUndefined();
    expect(result.version).toBe(8);
    expect(result.previousFingerprint).toBe(fingerprint("postgres://old"));
    expect(stored.value).not.toBe("postgres://old");
    expect(stored.value).toMatch(/^db_/);
  });

  it("rotates only due secrets", async () => {
    const { service } = buildService("2026-07-16T00:00:00.000Z");

    await expect(service.rotateDueSecrets()).resolves.toEqual([]);
  });

  it("marks secrets without rotation metadata as due", async () => {
    const { service } = buildService(undefined);

    await expect(service.rotateDueSecrets()).resolves.toHaveLength(1);
  });

  it("throws a 404 for missing secrets", async () => {
    const { service } = buildService();

    await expect(service.rotateSecret("missing")).rejects.toMatchObject({ statusCode: 404 });
  });

  it("generates type-specific API keys", () => {
    expect(generateSecretValue(SECRET_TYPES.API_KEY)).toMatch(/^api_/);
  });
});

describe("secret rotation routes", () => {
  it("exposes monitoring metrics for alerting dashboards", async () => {
    const res = await request(app).get("/internal/secrets/metrics");

    expect(res.status).toBe(200);
    expect(res.body.p99_target_ms).toBe(100);
  });

  it("rotates a configured secret via the internal endpoint", async () => {
    const res = await request(app).post("/internal/secrets/rotate/database.primary");

    expect(res.status).toBe(202);
    expect(res.body.name).toBe("database.primary");
    expect(res.body.value).toBeUndefined();
    expect(res.body.previousFingerprint).toHaveLength(64);
  });
});
