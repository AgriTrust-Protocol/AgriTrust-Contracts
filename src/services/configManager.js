"use strict";

const fs = require("fs");
const path = require("path");
const EventEmitter = require("events");

const DEFAULT_CONFIG = Object.freeze({
  rateLimit: Object.freeze({ capacity: 60, refillPerSecond: 30 }),
  features: Object.freeze({ shedCapacity: false, mutationEndpoints: true, escrowRead: true }),
  capacity: Object.freeze({ maxInFlight: null }),
  observability: Object.freeze({ hotReloadEnabled: true }),
});

const METRICS = {
  reloadAttempts: 0,
  reloadSuccess: 0,
  reloadFailures: 0,
  validationFailures: 0,
  lastReloadAt: null,
  lastError: null,
};

function clone(value) {
  return JSON.parse(JSON.stringify(value));
}

function mergeConfig(base, override = {}) {
  return {
    ...base,
    ...override,
    rateLimit: { ...base.rateLimit, ...(override.rateLimit || {}) },
    features: { ...base.features, ...(override.features || {}) },
    capacity: { ...base.capacity, ...(override.capacity || {}) },
    observability: { ...base.observability, ...(override.observability || {}) },
  };
}

function parseBoolean(value, name) {
  if (typeof value === "boolean") return value;
  throw new Error(`${name} must be a boolean`);
}

function parsePositiveNumber(value, name) {
  if (!Number.isFinite(value) || value <= 0) {
    throw new Error(`${name} must be a positive number`);
  }
  return value;
}

function validateConfig(input) {
  const config = mergeConfig(clone(DEFAULT_CONFIG), input || {});

  config.rateLimit.capacity = parsePositiveNumber(config.rateLimit.capacity, "rateLimit.capacity");
  config.rateLimit.refillPerSecond = parsePositiveNumber(
    config.rateLimit.refillPerSecond,
    "rateLimit.refillPerSecond"
  );

  for (const name of ["shedCapacity", "mutationEndpoints", "escrowRead"]) {
    config.features[name] = parseBoolean(config.features[name], `features.${name}`);
  }

  if (config.capacity.maxInFlight !== null) {
    config.capacity.maxInFlight = parsePositiveNumber(config.capacity.maxInFlight, "capacity.maxInFlight");
  }

  config.observability.hotReloadEnabled = parseBoolean(
    config.observability.hotReloadEnabled,
    "observability.hotReloadEnabled"
  );

  return Object.freeze(config);
}

function loadConfigFile(configPath, fsImpl = fs) {
  const body = fsImpl.readFileSync(configPath, "utf8");
  return JSON.parse(body);
}

class ConfigManager extends EventEmitter {
  constructor(options = {}) {
    super();
    this.configPath = options.configPath || process.env.AGRITRUST_CONFIG_PATH || path.join(process.cwd(), "config", "runtime.json");
    this.fs = options.fs || fs;
    this.reloadDelayMs = options.reloadDelayMs || 25;
    this.config = validateConfig(options.initialConfig || {});
    this.watcher = null;
    this.reloadTimer = null;
  }

  current() {
    return this.config;
  }

  reload() {
    METRICS.reloadAttempts += 1;
    try {
      const next = validateConfig(loadConfigFile(this.configPath, this.fs));
      this.config = next;
      METRICS.reloadSuccess += 1;
      METRICS.lastReloadAt = new Date().toISOString();
      METRICS.lastError = null;
      this.emit("reload", next);
      return { ok: true, config: next };
    } catch (err) {
      METRICS.reloadFailures += 1;
      if (/must be|Unexpected token|JSON/.test(err.message)) METRICS.validationFailures += 1;
      METRICS.lastError = err.message;
      this.emit("reload_error", err);
      return { ok: false, error: err.message, config: this.config };
    }
  }

  start() {
    if (this.watcher || this.current().observability.hotReloadEnabled === false) return;
    this.reload();
    this.watcher = this.fs.watch(this.configPath, { persistent: false }, () => {
      clearTimeout(this.reloadTimer);
      this.reloadTimer = setTimeout(() => this.reload(), this.reloadDelayMs);
    });
  }

  stop() {
    clearTimeout(this.reloadTimer);
    this.reloadTimer = null;
    if (this.watcher) this.watcher.close();
    this.watcher = null;
  }

  metrics() {
    return { ...METRICS, watched_path: this.configPath, hot_reload_active: Boolean(this.watcher) };
  }
}

function resetConfigMetrics() {
  METRICS.reloadAttempts = 0;
  METRICS.reloadSuccess = 0;
  METRICS.reloadFailures = 0;
  METRICS.validationFailures = 0;
  METRICS.lastReloadAt = null;
  METRICS.lastError = null;
}

module.exports = { ConfigManager, DEFAULT_CONFIG, resetConfigMetrics, validateConfig };
