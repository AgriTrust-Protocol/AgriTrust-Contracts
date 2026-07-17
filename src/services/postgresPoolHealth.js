"use strict";

const DEFAULTS = Object.freeze({
  min: Number(process.env.PG_POOL_MIN || 2),
  max: Number(process.env.PG_POOL_MAX || 10),
  floor: Number(process.env.PG_POOL_FLOOR || 2),
  ceiling: Number(process.env.PG_POOL_CEILING || 50),
  acquireTimeoutMs: Number(process.env.PG_POOL_ACQUIRE_TIMEOUT_MS || 75),
  probeTimeoutMs: Number(process.env.PG_POOL_PROBE_TIMEOUT_MS || 75),
  targetP99Ms: Number(process.env.PG_POOL_TARGET_P99_MS || 100),
  scaleUpAtUtilization: Number(process.env.PG_POOL_SCALE_UP_UTILIZATION || 0.85),
  scaleDownAtUtilization: Number(process.env.PG_POOL_SCALE_DOWN_UTILIZATION || 0.35),
  scaleStep: Number(process.env.PG_POOL_SCALE_STEP || 2),
});

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value));
}

function withTimeout(promise, timeoutMs, message) {
  let timeout;
  const timer = new Promise((_resolve, reject) => {
    timeout = setTimeout(() => reject(new Error(message)), timeoutMs);
  });
  return Promise.race([promise, timer]).finally(() => clearTimeout(timeout));
}

class AdaptivePostgresPool {
  constructor({ Pool, config = {}, logger = console } = {}) {
    this.Pool = Pool;
    this.config = { ...DEFAULTS, ...config };
    this.logger = logger;
    this.currentMax = clamp(this.config.max, this.config.floor, this.config.ceiling);
    this.pool = this.createPool(this.currentMax);
    this.lastProbe = null;
    this.metrics = {
      probes_total: 0,
      probe_failures_total: 0,
      resizes_total: 0,
      current_max: this.currentMax,
      last_latency_ms: null,
    };
  }

  createPool(max) {
    if (!this.Pool) {
      return null;
    }
    return new this.Pool({ ...this.config, max, min: Math.min(this.config.min, max) });
  }

  isConfigured() {
    return Boolean(this.pool);
  }

  snapshot() {
    const pool = this.pool || {};
    return {
      totalCount: pool.totalCount || 0,
      idleCount: pool.idleCount || 0,
      waitingCount: pool.waitingCount || 0,
      max: this.currentMax,
      utilization: this.currentMax > 0 ? (pool.totalCount || 0) / this.currentMax : 0,
    };
  }

  desiredSize({ utilization, waitingCount, latencyMs }) {
    if (waitingCount > 0 || utilization >= this.config.scaleUpAtUtilization || latencyMs >= this.config.targetP99Ms) {
      return clamp(this.currentMax + this.config.scaleStep, this.config.floor, this.config.ceiling);
    }
    if (waitingCount === 0 && utilization <= this.config.scaleDownAtUtilization) {
      return clamp(this.currentMax - this.config.scaleStep, this.config.floor, this.config.ceiling);
    }
    return this.currentMax;
  }

  async resize(max) {
    if (max === this.currentMax || !this.Pool) {
      return false;
    }
    const previous = this.pool;
    this.currentMax = max;
    this.pool = this.createPool(max);
    this.metrics.current_max = max;
    this.metrics.resizes_total += 1;
    if (previous && typeof previous.end === "function") {
      previous.end().catch((err) => this.logger.warn("postgres pool drain failed", err.message));
    }
    return true;
  }

  async probe() {
    const started = Date.now();
    this.metrics.probes_total += 1;
    if (!this.pool) {
      this.lastProbe = { ok: false, status: "disabled", latency_ms: 0, error: "PostgreSQL pool is not configured" };
      return this.lastProbe;
    }

    let client;
    try {
      client = await withTimeout(this.pool.connect(), this.config.acquireTimeoutMs, "PostgreSQL pool acquire timeout");
      await withTimeout(client.query("SELECT 1"), this.config.probeTimeoutMs, "PostgreSQL health query timeout");
      const latencyMs = Date.now() - started;
      const snapshot = this.snapshot();
      await this.resize(this.desiredSize({ ...snapshot, latencyMs }));
      this.metrics.last_latency_ms = latencyMs;
      this.lastProbe = { ok: true, status: "ok", latency_ms: latencyMs, pool: this.snapshot() };
      return this.lastProbe;
    } catch (err) {
      const latencyMs = Date.now() - started;
      this.metrics.probe_failures_total += 1;
      this.metrics.last_latency_ms = latencyMs;
      this.lastProbe = { ok: false, status: "degraded", latency_ms: latencyMs, error: err.message, pool: this.snapshot() };
      return this.lastProbe;
    } finally {
      if (client && typeof client.release === "function") {
        client.release();
      }
    }
  }

  prometheusMetrics() {
    return [
      `agritrust_postgres_pool_max ${this.metrics.current_max}`,
      `agritrust_postgres_pool_probes_total ${this.metrics.probes_total}`,
      `agritrust_postgres_pool_probe_failures_total ${this.metrics.probe_failures_total}`,
      `agritrust_postgres_pool_resizes_total ${this.metrics.resizes_total}`,
      `agritrust_postgres_pool_last_latency_ms ${this.metrics.last_latency_ms || 0}`,
    ].join("\n");
  }
}

function buildDefaultPool({ Pool } = {}) {
  if (!Pool) {
    return new AdaptivePostgresPool();
  }
  return new AdaptivePostgresPool({ Pool, config: { connectionString: process.env.DATABASE_URL } });
}

module.exports = { AdaptivePostgresPool, buildDefaultPool, DEFAULTS };
