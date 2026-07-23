"use strict";

const DEFAULT_TTL_MS = 5 * 60 * 1000;
const DEFAULT_MAX_SIZE = 5000;
const CLEANUP_INTERVAL_MS = 60 * 1000;
const STALE_THRESHOLD_MS = 24 * 60 * 60 * 1000;

function nowMs() {
  return Date.now();
}

function stableKey(parts) {
  return parts
    .map((p) => (typeof p === "object" ? JSON.stringify(p, Object.keys(p).sort()) : String(p)))
    .join("::");
}

class CacheEntry {
  constructor(value, ttl) {
    this.value = value;
    this.ttl = ttl;
    this.createdAt = nowMs();
    this.accessCount = 0;
  }

  get isExpired() {
    return nowMs() - this.createdAt >= this.ttl;
  }

  get age() {
    return nowMs() - this.createdAt;
  }
}

class InMemoryCacheLayer {
  constructor(options = {}) {
    this.ttl = options.ttl ?? DEFAULT_TTL_MS;
    this.maxSize = options.maxSize ?? DEFAULT_MAX_SIZE;
    this.store = new Map();
    this.hits = 0;
    this.misses = 0;
    this.evictions = 0;

    this._cleanupTimer = setInterval(() => this._evictStale(), CLEANUP_INTERVAL_MS);
    if (this._cleanupTimer.unref) this._cleanupTimer.unref();
  }

  get(key) {
    const entry = this.store.get(key);
    if (!entry) {
      this.misses += 1;
      return undefined;
    }
    if (entry.isExpired) {
      this.store.delete(key);
      this.misses += 1;
      return undefined;
    }
    entry.accessCount += 1;
    this.hits += 1;
    return entry.value;
  }

  set(key, value, ttl) {
    if (this.store.size >= this.maxSize) {
      this._evictOne();
    }
    const effectiveTtl = ttl ?? this.ttl;
    this.store.set(key, new CacheEntry(value, effectiveTtl));
  }

  has(key) {
    const entry = this.store.get(key);
    if (!entry) return false;
    if (entry.isExpired) {
      this.store.delete(key);
      return false;
    }
    return true;
  }

  delete(key) {
    return this.store.delete(key);
  }

  clear() {
    this.store.clear();
    this.hits = 0;
    this.misses = 0;
    this.evictions = 0;
  }

  get size() {
    return this.store.size;
  }

  get stats() {
    const total = this.hits + this.misses;
    return {
      size: this.store.size,
      maxSize: this.maxSize,
      defaultTtlMs: this.ttl,
      hits: this.hits,
      misses: this.misses,
      evictions: this.evictions,
      hitRate: total > 0 ? (this.hits / total) : 0,
    };
  }

  getOrSet(key, factory, ttl) {
    const existing = this.get(key);
    if (existing !== undefined) return existing;
    const value = factory();
    this.set(key, value, ttl);
    return value;
  }

  async getOrSetAsync(key, factory, ttl) {
    const existing = this.get(key);
    if (existing !== undefined) return existing;
    const value = await factory();
    this.set(key, value, ttl);
    return value;
  }

  mget(keys) {
    return keys.map((key) => this.get(key));
  }

  mset(entries, ttl) {
    for (const [key, value] of entries) {
      this.set(key, value, ttl);
    }
  }

  _evictOne() {
    let oldestKey = null;
    let oldestAccess = Infinity;
    for (const [key, entry] of this.store) {
      if (entry.accessCount < oldestAccess) {
        oldestAccess = entry.accessCount;
        oldestKey = key;
      }
    }
    if (oldestKey) {
      this.store.delete(oldestKey);
      this.evictions += 1;
    }
  }

  _evictStale() {
    const now = nowMs();
    for (const [key, entry] of this.store) {
      if (entry.isExpired) {
        this.store.delete(key);
        this.evictions += 1;
      }
    }
  }

  destroy() {
    if (this._cleanupTimer) {
      clearInterval(this._cleanupTimer);
      this._cleanupTimer = null;
    }
    this.clear();
  }
}

class RedisCacheLayer {
  constructor(redisClient, options = {}) {
    if (!redisClient) throw new Error("Redis client is required");
    this.client = redisClient;
    this.prefix = options.prefix ?? "agritrust:cache:";
    this.defaultTtl = options.ttl ?? DEFAULT_TTL_MS;
    this.hits = 0;
    this.misses = 0;
  }

  _buildKey(key) {
    return `${this.prefix}${key}`;
  }

  async get(key) {
    const fullKey = this._buildKey(key);
    try {
      const raw = await this.client.get(fullKey);
      if (raw === null || raw === undefined) {
        this.misses += 1;
        return undefined;
      }
      const parsed = JSON.parse(raw);
      if (parsed._expires && nowMs() > parsed._expires) {
        await this.client.del(fullKey);
        this.misses += 1;
        return undefined;
      }
      this.hits += 1;
      return parsed._value;
    } catch {
      this.misses += 1;
      return undefined;
    }
  }

  async set(key, value, ttl) {
    const fullKey = this._buildKey(key);
    const effectiveTtl = ttl ?? this.defaultTtl;
    const entry = JSON.stringify({ _value: value, _expires: nowMs() + effectiveTtl });
    await this.client.set(fullKey, entry, "PX", effectiveTtl);
  }

  async has(key) {
    const fullKey = this._buildKey(key);
    const exists = await this.client.exists(fullKey);
    return exists === 1;
  }

  async delete(key) {
    const fullKey = this._buildKey(key);
    await this.client.del(fullKey);
  }

  async clear() {
    let cursor = "0";
    do {
      const [nextCursor, keys] = await this.client.scan(cursor, "MATCH", `${this.prefix}*`, "COUNT", 100);
      cursor = nextCursor;
      if (keys.length > 0) {
        await this.client.del(...keys);
      }
    } while (cursor !== "0");
    this.hits = 0;
    this.misses = 0;
  }

  async getOrSet(key, factory, ttl) {
    const existing = await this.get(key);
    if (existing !== undefined) return existing;
    const value = typeof factory === "function" ? factory() : factory;
    await this.set(key, value, ttl);
    return value;
  }

  async getOrSetAsync(key, factory, ttl) {
    const existing = await this.get(key);
    if (existing !== undefined) return existing;
    const value = await factory();
    await this.set(key, value, ttl);
    return value;
  }

  get stats() {
    const total = this.hits + this.misses;
    return {
      defaultTtlMs: this.defaultTtl,
      hits: this.hits,
      misses: this.misses,
      hitRate: total > 0 ? (this.hits / total) : 0,
      prefix: this.prefix,
    };
  }
}

module.exports = {
  InMemoryCacheLayer,
  RedisCacheLayer,
  stableKey,
};
