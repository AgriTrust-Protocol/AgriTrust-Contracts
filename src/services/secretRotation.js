/**
 * Secret rotation primitives for database credentials and API keys.
 *
 * The rotation service is intentionally dependency-injected so production can
 * plug in Vault/AWS Secrets Manager/KMS while tests use an in-memory provider.
 */

"use strict";

const crypto = require("crypto");

const SECRET_TYPES = Object.freeze({
  DATABASE: "database_credential",
  API_KEY: "api_key",
});

const DEFAULT_POLICY = Object.freeze({
  rotationIntervalMs: 30 * 24 * 60 * 60 * 1000,
  gracePeriodMs: 15 * 60 * 1000,
  maxCriticalPathMs: 100,
});

function nowIso(clock) {
  return new Date(clock()).toISOString();
}

function fingerprint(value) {
  return crypto.createHash("sha256").update(String(value)).digest("hex");
}

function generateSecretValue(type) {
  const prefix = type === SECRET_TYPES.DATABASE ? "db" : "api";
  return `${prefix}_${crypto.randomBytes(32).toString("base64url")}`;
}

class InMemorySecretProvider {
  constructor(initialSecrets = []) {
    this.secrets = new Map();
    initialSecrets.forEach((secret) => this.putSecret(secret));
  }

  async listSecrets() {
    return Array.from(this.secrets.values()).map((secret) => ({ ...secret }));
  }

  async getSecret(name) {
    const secret = this.secrets.get(name);
    return secret ? { ...secret } : null;
  }

  async putSecret(secret) {
    if (!secret || !secret.name || !secret.type || !secret.value) {
      throw new Error("Secret must include name, type, and value");
    }
    this.secrets.set(secret.name, { ...secret });
    return { ...this.secrets.get(secret.name) };
  }

  async promoteSecret(name, nextValue, metadata = {}) {
    const current = await this.getSecret(name);
    if (!current) {
      const err = new Error(`Secret not found: ${name}`);
      err.statusCode = 404;
      throw err;
    }

    const rotated = {
      ...current,
      value: nextValue,
      version: (current.version || 0) + 1,
      previousFingerprint: fingerprint(current.value),
      fingerprint: fingerprint(nextValue),
      rotatedAt: metadata.rotatedAt,
      expiresAt: metadata.expiresAt,
      status: "active",
    };
    return this.putSecret(rotated);
  }
}

class SecretRotationService {
  constructor({ provider, policy = {}, clock = Date.now, logger = console } = {}) {
    if (!provider) {
      throw new Error("SecretRotationService requires a provider");
    }
    this.provider = provider;
    this.policy = { ...DEFAULT_POLICY, ...policy };
    this.clock = clock;
    this.logger = logger;
  }

  async getSecret(name) {
    const startedAt = this.clock();
    const secret = await this.provider.getSecret(name);
    const durationMs = this.clock() - startedAt;
    if (durationMs > this.policy.maxCriticalPathMs) {
      this.logger.warn("secret_read_slow", { name, durationMs });
    }
    return secret;
  }

  async rotateSecret(name) {
    const current = await this.provider.getSecret(name);
    if (!current) {
      const err = new Error(`Secret not found: ${name}`);
      err.statusCode = 404;
      throw err;
    }

    const rotatedAt = nowIso(this.clock);
    const expiresAt = new Date(this.clock() + this.policy.gracePeriodMs).toISOString();
    const nextValue = generateSecretValue(current.type);
    const rotated = await this.provider.promoteSecret(name, nextValue, { rotatedAt, expiresAt });

    this.logger.info("secret_rotated", {
      name,
      type: current.type,
      version: rotated.version,
      rotatedAt,
      previousFingerprint: rotated.previousFingerprint,
    });

    return this.redact(rotated);
  }

  async rotateDueSecrets() {
    const secrets = await this.provider.listSecrets();
    const due = secrets.filter((secret) => this.isDue(secret));
    const rotated = [];
    for (const secret of due) {
      rotated.push(await this.rotateSecret(secret.name));
    }
    return rotated;
  }

  isDue(secret) {
    if (!secret.rotatedAt) return true;
    return this.clock() - new Date(secret.rotatedAt).getTime() >= this.policy.rotationIntervalMs;
  }

  redact(secret) {
    if (!secret) return null;
    const { value, ...safe } = secret;
    return safe;
  }

  metrics() {
    return {
      rotation_interval_ms: this.policy.rotationIntervalMs,
      grace_period_ms: this.policy.gracePeriodMs,
      p99_target_ms: this.policy.maxCriticalPathMs,
    };
  }
}

module.exports = {
  DEFAULT_POLICY,
  SECRET_TYPES,
  InMemorySecretProvider,
  SecretRotationService,
  fingerprint,
  generateSecretValue,
};
