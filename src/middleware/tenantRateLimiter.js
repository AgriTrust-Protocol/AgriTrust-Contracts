/**
 * tenantRateLimiter.js
 * ────────────────────
 * Express middleware implementing per-tenant token-bucket rate limiting.
 *
 * The implementation is intentionally in-memory for the local API process. It
 * provides deterministic O(1) checks for critical paths and exposes snapshots
 * that can be exported to metrics collectors by production wiring.
 */

"use strict";

const DEFAULT_CAPACITY = 60;
const DEFAULT_REFILL_PER_SECOND = 30;
const DEFAULT_TENANT = "anonymous";
const MAX_TENANT_ID_LENGTH = 128;
const TENANT_ID_PATTERN = /^[A-Za-z0-9_.:-]+$/;

class TokenBucket {
  constructor({ capacity, refillPerSecond, now }) {
    if (!Number.isFinite(capacity) || capacity <= 0) {
      throw new Error("capacity must be a positive number");
    }
    if (!Number.isFinite(refillPerSecond) || refillPerSecond <= 0) {
      throw new Error("refillPerSecond must be a positive number");
    }

    this.capacity = capacity;
    this.refillPerSecond = refillPerSecond;
    this.tokens = capacity;
    this.lastRefillMs = now();
  }

  refill(nowMs) {
    const elapsedMs = Math.max(0, nowMs - this.lastRefillMs);
    if (elapsedMs === 0) {
      return;
    }

    const tokensToAdd = (elapsedMs / 1000) * this.refillPerSecond;
    this.tokens = Math.min(this.capacity, this.tokens + tokensToAdd);
    this.lastRefillMs = nowMs;
  }

  consume(cost, nowMs) {
    this.refill(nowMs);

    if (this.tokens < cost) {
      return {
        allowed: false,
        remaining: Math.floor(this.tokens),
        retryAfterSeconds: Math.ceil((cost - this.tokens) / this.refillPerSecond),
      };
    }

    this.tokens -= cost;
    return {
      allowed: true,
      remaining: Math.floor(this.tokens),
      retryAfterSeconds: 0,
    };
  }

  snapshot(nowMs) {
    this.refill(nowMs);
    return {
      capacity: this.capacity,
      refill_per_second: this.refillPerSecond,
      remaining: Math.floor(this.tokens),
    };
  }
}

function normaliseTenantId(rawTenantId) {
  const tenantId = typeof rawTenantId === "string" && rawTenantId.trim()
    ? rawTenantId.trim()
    : DEFAULT_TENANT;

  if (tenantId.length > MAX_TENANT_ID_LENGTH || !TENANT_ID_PATTERN.test(tenantId)) {
    const err = new Error("Invalid tenant id");
    err.statusCode = 400;
    throw err;
  }

  return tenantId;
}

function createTenantRateLimiter(options = {}) {
  const capacity = Number(options.capacity || process.env.RATE_LIMIT_CAPACITY || DEFAULT_CAPACITY);
  const refillPerSecond = Number(
    options.refillPerSecond || process.env.RATE_LIMIT_REFILL_PER_SECOND || DEFAULT_REFILL_PER_SECOND
  );
  const cost = Number(options.cost || 1);
  const now = options.now || Date.now;
  const buckets = options.buckets || new Map();
  const tenantIdResolver = options.tenantIdResolver || ((req) => req.get("x-tenant-id"));

  function getBucket(tenantId) {
    if (!buckets.has(tenantId)) {
      buckets.set(tenantId, new TokenBucket({ capacity, refillPerSecond, now }));
    }
    return buckets.get(tenantId);
  }

  function middleware(req, res, next) {
    let tenantId;
    try {
      tenantId = normaliseTenantId(tenantIdResolver(req));
    } catch (err) {
      return res.status(err.statusCode || 400).json({ error: err.message });
    }

    const result = getBucket(tenantId).consume(cost, now());
    res.set("X-RateLimit-Limit", String(capacity));
    res.set("X-RateLimit-Remaining", String(result.remaining));
    res.set("X-RateLimit-Tenant", tenantId);

    if (!result.allowed) {
      res.set("Retry-After", String(Math.max(1, result.retryAfterSeconds)));
      return res.status(429).json({ error: "Rate limit exceeded" });
    }

    return next();
  }

  middleware.snapshot = () => {
    const nowMs = now();
    return Array.from(buckets.entries()).map(([tenant_id, bucket]) => ({
      tenant_id,
      ...bucket.snapshot(nowMs),
    }));
  };
  middleware.reset = () => buckets.clear();

  return middleware;
}

module.exports = {
  TokenBucket,
  createTenantRateLimiter,
  normaliseTenantId,
};
