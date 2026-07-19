"use strict";

const crypto = require("crypto");

const DEFAULT_RETENTION_MS = 14 * 24 * 60 * 60 * 1000;
const DEFAULT_MAX_REASON_LENGTH = 256;
const DEFAULT_MAX_ERROR_LENGTH = 2_048;

const redactedKeys = new Set(["authorization", "cookie", "secret", "token", "password", "apiKey", "apikey"]);

function nowIso(now = Date.now()) {
  return new Date(now).toISOString();
}

function redact(value) {
  if (Array.isArray(value)) return value.map(redact);
  if (!value || typeof value !== "object") return value;
  return Object.fromEntries(Object.entries(value).map(([key, val]) => [key, redactedKeys.has(key.toLowerCase()) ? "[REDACTED]" : redact(val)]));
}

function truncate(value, maxLength) {
  const text = String(value || "");
  return text.length > maxLength ? `${text.slice(0, maxLength)}…` : text;
}

function stableStringify(value) {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(stableStringify).join(",")}]`;
  return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${stableStringify(value[key])}`).join(",")}}`;
}

function fingerprint({ service, messageId, payload }) {
  return crypto.createHash("sha256").update(`${service}:${messageId}:${stableStringify(payload)}`).digest("hex");
}

class InMemoryDeadLetterQueue {
  constructor(options = {}) {
    this.retentionMs = options.retentionMs ?? DEFAULT_RETENTION_MS;
    this.maxReasonLength = options.maxReasonLength ?? DEFAULT_MAX_REASON_LENGTH;
    this.maxErrorLength = options.maxErrorLength ?? DEFAULT_MAX_ERROR_LENGTH;
    this.entries = new Map();
    this.metrics = { enqueued: 0, replayed: 0, purged: 0 };
  }

  enqueue({ service, queue, messageId, payload, reason, error, attempts = 0, metadata = {}, now = Date.now() }) {
    if (!service || !messageId) throw new Error("DLQ service and messageId are required");
    const safePayload = redact(payload);
    const id = fingerprint({ service, messageId, payload: safePayload });
    const entry = {
      id,
      service,
      queue: queue || "default",
      messageId,
      payload: safePayload,
      reason: truncate(reason || "processing_failed", this.maxReasonLength),
      error: truncate(error?.message || error || "", this.maxErrorLength),
      attempts,
      metadata: redact(metadata),
      createdAt: nowIso(now),
      expiresAt: nowIso(now + this.retentionMs),
      replayedAt: null,
    };
    this.entries.set(id, entry);
    this.metrics.enqueued += 1;
    return { ...entry };
  }

  get(id) {
    const entry = this.entries.get(id);
    return entry ? { ...entry } : null;
  }

  list({ service, includeReplayed = false } = {}) {
    return Array.from(this.entries.values())
      .filter((entry) => (!service || entry.service === service) && (includeReplayed || !entry.replayedAt))
      .map((entry) => ({ ...entry }));
  }

  async replay(id, handler, { now = Date.now() } = {}) {
    if (typeof handler !== "function") throw new Error("A replay handler is required");
    const entry = this.entries.get(id);
    if (!entry) return { ok: false, error: "not_found" };
    if (entry.replayedAt) return { ok: false, error: "already_replayed" };
    await handler({ ...entry, payload: redact(entry.payload) });
    entry.replayedAt = nowIso(now);
    this.metrics.replayed += 1;
    return { ok: true, entry: { ...entry } };
  }

  purgeExpired({ now = Date.now() } = {}) {
    let purged = 0;
    for (const [id, entry] of this.entries.entries()) {
      if (Date.parse(entry.expiresAt) <= now) {
        this.entries.delete(id);
        purged += 1;
      }
    }
    this.metrics.purged += purged;
    return purged;
  }

  snapshotMetrics() {
    const pending = this.list().length;
    return { ...this.metrics, pending, totalStored: this.entries.size };
  }

  reset() {
    this.entries.clear();
    this.metrics = { enqueued: 0, replayed: 0, purged: 0 };
  }
}

module.exports = { InMemoryDeadLetterQueue, redact };
