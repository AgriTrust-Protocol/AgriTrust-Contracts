"use strict";

const crypto = require("crypto");

const DEFAULT_SENSITIVE_PATTERNS = [/secret/i, /password/i, /token/i, /key/i, /credential/i];

function canonicalize(value) {
  if (Array.isArray(value)) return value.map(canonicalize);
  if (value && typeof value === "object") {
    return Object.keys(value).sort().reduce((acc, key) => {
      acc[key] = canonicalize(value[key]);
      return acc;
    }, {});
  }
  return value;
}

function fingerprint(value) {
  return crypto.createHash("sha256").update(JSON.stringify(canonicalize(value))).digest("hex");
}

function redactConfig(config, patterns = DEFAULT_SENSITIVE_PATTERNS) {
  return Object.keys(config).sort().reduce((acc, key) => {
    const sensitive = patterns.some((pattern) => pattern.test(key));
    acc[key] = sensitive ? "[REDACTED]" : config[key];
    return acc;
  }, {});
}

class RuntimeConfigAuditor {
  constructor({ baseline = {}, now = () => new Date(), sensitivePatterns = DEFAULT_SENSITIVE_PATTERNS } = {}) {
    this.baseline = canonicalize(baseline);
    this.baselineHash = fingerprint(this.baseline);
    this.now = now;
    this.sensitivePatterns = sensitivePatterns;
    this.history = [];
    this.counters = { audits: 0, drift_detected: 0 };
  }

  audit(currentConfig, metadata = {}) {
    const current = canonicalize(currentConfig || {});
    const drift = this.diff(this.baseline, current);
    const driftDetected = drift.length > 0;
    const event = {
      checked_at: this.now().toISOString(),
      service: metadata.service || "api",
      baseline_hash: this.baselineHash,
      current_hash: fingerprint(current),
      drift_detected: driftDetected,
      drift,
      config: redactConfig(current, this.sensitivePatterns),
    };

    this.counters.audits += 1;
    if (driftDetected) this.counters.drift_detected += 1;
    this.history.push(event);
    return event;
  }

  diff(expected, actual) {
    const keys = new Set([...Object.keys(expected), ...Object.keys(actual)]);
    return Array.from(keys).sort().reduce((changes, key) => {
      if (!(key in actual)) changes.push({ key, type: "missing", expected: expected[key] });
      else if (!(key in expected)) changes.push({ key, type: "unexpected", actual: actual[key] });
      else if (JSON.stringify(expected[key]) !== JSON.stringify(actual[key])) {
        changes.push({ key, type: "changed", expected: expected[key], actual: actual[key] });
      }
      return changes;
    }, []);
  }

  metrics() {
    const last = this.history[this.history.length - 1];
    return {
      config_audit_total: this.counters.audits,
      config_drift_detected_total: this.counters.drift_detected,
      config_drift_active: last && last.drift_detected ? 1 : 0,
      config_baseline_hash: this.baselineHash,
    };
  }
}

function loadRuntimeConfig(env = process.env) {
  return {
    node_env: env.NODE_ENV || "development",
    port: Number(env.PORT || 3000),
    rate_limit_capacity: Number(env.RATE_LIMIT_CAPACITY || 60),
    rate_limit_refill_per_second: Number(env.RATE_LIMIT_REFILL_PER_SECOND || 30),
    capacity_shed_max_in_flight: env.CAPACITY_SHED_MAX_IN_FLIGHT || "unbounded",
    feature_shed_capacity: env.FEATURE_SHED_CAPACITY || "false",
    feature_escrow_mutations: env.FEATURE_ESCROW_MUTATIONS || "true",
    feature_escrow_read: env.FEATURE_ESCROW_READ || "true",
  };
}

function createDefaultConfigAuditor(env = process.env) {
  return new RuntimeConfigAuditor({ baseline: loadRuntimeConfig(env) });
}

module.exports = { RuntimeConfigAuditor, createDefaultConfigAuditor, fingerprint, loadRuntimeConfig, redactConfig };
