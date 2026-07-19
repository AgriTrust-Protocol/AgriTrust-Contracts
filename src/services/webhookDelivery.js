"use strict";

const crypto = require("crypto");
const { InMemoryDeadLetterQueue } = require("./deadLetterQueue");

const DEFAULT_MAX_ATTEMPTS = 5;
const DEFAULT_INITIAL_DELAY_MS = 100;
const DEFAULT_BACKOFF_FACTOR = 2;
const DEFAULT_MAX_DELAY_MS = 5_000;
const DEFAULT_TIMEOUT_MS = 5_000;
const MAX_CLOCK_SKEW_SECONDS = 300;

const metrics = {
  enqueued: 0,
  delivered: 0,
  failed: 0,
  attempts: 0,
  signatureVerificationFailed: 0,
  deliveryLatencyMs: [],
  deadLettered: 0,
};

const webhookDeadLetterQueue = new InMemoryDeadLetterQueue();

function stableStringify(value) {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(stableStringify).join(",")}]`;
  return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${stableStringify(value[key])}`).join(",")}}`;
}

function payloadToString(payload) {
  return typeof payload === "string" || Buffer.isBuffer(payload) ? payload.toString() : stableStringify(payload);
}

function computeSignature({ secret, timestamp, payload }) {
  if (!secret || typeof secret !== "string") throw new Error("Webhook secret is required");
  return crypto.createHmac("sha256", secret).update(`${timestamp}.${payloadToString(payload)}`).digest("hex");
}

function timingSafeEqualHex(a, b) {
  if (typeof a !== "string" || typeof b !== "string") return false;
  const left = Buffer.from(a, "hex");
  const right = Buffer.from(b, "hex");
  return left.length === right.length && crypto.timingSafeEqual(left, right);
}

function buildSignatureHeaders({ secret, payload, timestamp = Math.floor(Date.now() / 1000), eventId }) {
  const signature = computeSignature({ secret, timestamp, payload });
  return {
    "x-agritrust-webhook-timestamp": String(timestamp),
    "x-agritrust-webhook-signature": `sha256=${signature}`,
    ...(eventId ? { "x-agritrust-webhook-id": eventId } : {}),
  };
}

function verifySignature({ secret, payload, timestamp, signature, now = Math.floor(Date.now() / 1000), toleranceSeconds = MAX_CLOCK_SKEW_SECONDS }) {
  const parsed = Number(timestamp);
  const digest = typeof signature === "string" && signature.startsWith("sha256=") ? signature.slice(7) : signature;
  if (!Number.isInteger(parsed) || Math.abs(now - parsed) > toleranceSeconds) {
    metrics.signatureVerificationFailed += 1;
    return false;
  }
  const expected = computeSignature({ secret, timestamp: parsed, payload });
  const ok = timingSafeEqualHex(digest, expected);
  if (!ok) metrics.signatureVerificationFailed += 1;
  return ok;
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function nextDelayMs(attempt, options) {
  const base = options.initialDelayMs ?? DEFAULT_INITIAL_DELAY_MS;
  const factor = options.backoffFactor ?? DEFAULT_BACKOFF_FACTOR;
  const maxDelay = options.maxDelayMs ?? DEFAULT_MAX_DELAY_MS;
  return Math.min(maxDelay, Math.floor(base * factor ** (attempt - 1)));
}

function assertEndpoint(endpoint) {
  try {
    const url = new URL(endpoint.url);
    if (url.protocol !== "https:" && process.env.NODE_ENV !== "test") throw new Error("Webhook URLs must use HTTPS");
  } catch (err) {
    const error = new Error(`Invalid webhook endpoint: ${err.message}`);
    error.statusCode = 400;
    throw error;
  }
  if (!endpoint.secret || endpoint.secret.length < 16) {
    const error = new Error("Webhook endpoint secret must be at least 16 characters");
    error.statusCode = 400;
    throw error;
  }
}

async function sendAttempt({ endpoint, event, fetchImpl, timeoutMs }) {
  const controller = typeof AbortController !== "undefined" ? new AbortController() : null;
  const timer = controller ? setTimeout(() => controller.abort(), timeoutMs) : null;
  const payload = { id: event.id, type: event.type, created_at: event.created_at, data: event.data };
  const headers = {
    "content-type": "application/json",
    "user-agent": "AgriTrust-Webhook/1.0",
    ...buildSignatureHeaders({ secret: endpoint.secret, payload, eventId: event.id }),
  };
  try {
    const response = await fetchImpl(endpoint.url, {
      method: "POST",
      headers,
      body: payloadToString(payload),
      signal: controller?.signal,
    });
    return response.status >= 200 && response.status < 300;
  } finally {
    if (timer) clearTimeout(timer);
  }
}

async function deliverWebhook(endpoint, event, options = {}) {
  assertEndpoint(endpoint);
  if (!event || !event.id || !event.type) throw new Error("Webhook event id and type are required");
  const fetchImpl = options.fetchImpl || global.fetch;
  if (typeof fetchImpl !== "function") throw new Error("A fetch implementation is required");

  const maxAttempts = options.maxAttempts ?? DEFAULT_MAX_ATTEMPTS;
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const started = Date.now();
  metrics.enqueued += 1;

  for (let attempt = 1; attempt <= maxAttempts; attempt += 1) {
    metrics.attempts += 1;
    try {
      if (await sendAttempt({ endpoint, event, fetchImpl, timeoutMs })) {
        metrics.delivered += 1;
        metrics.deliveryLatencyMs.push(Date.now() - started);
        return { ok: true, attempts: attempt };
      }
    } catch (_err) {
      // Retry network and timeout failures without leaking endpoint secrets.
    }
    if (attempt < maxAttempts) await delay(options.disableDelay ? 0 : nextDelayMs(attempt, options));
  }
  metrics.failed += 1;
  metrics.deadLettered += 1;
  metrics.deliveryLatencyMs.push(Date.now() - started);
  const dlqEntry = webhookDeadLetterQueue.enqueue({
    service: "webhook-delivery",
    queue: endpoint.id || endpoint.url,
    messageId: event.id,
    payload: { type: event.type, created_at: event.created_at, data: event.data },
    reason: "max_attempts_exhausted",
    attempts: maxAttempts,
    metadata: { endpointUrl: endpoint.url },
  });
  return { ok: false, attempts: maxAttempts, deadLetterId: dlqEntry.id };
}

function snapshotMetrics() {
  const samples = [...metrics.deliveryLatencyMs].sort((a, b) => a - b);
  const p99Index = samples.length ? Math.min(samples.length - 1, Math.ceil(samples.length * 0.99) - 1) : 0;
  return { ...metrics, p99DeliveryLatencyMs: samples[p99Index] || 0, deadLetterQueue: webhookDeadLetterQueue.snapshotMetrics() };
}

function resetMetrics() {
  Object.assign(metrics, { enqueued: 0, delivered: 0, failed: 0, attempts: 0, signatureVerificationFailed: 0, deliveryLatencyMs: [], deadLettered: 0 });
  webhookDeadLetterQueue.reset();
}

module.exports = { buildSignatureHeaders, computeSignature, deliverWebhook, nextDelayMs, resetMetrics, snapshotMetrics, verifySignature, webhookDeadLetterQueue };
