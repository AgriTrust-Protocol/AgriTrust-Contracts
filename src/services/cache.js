"use strict";

const net = require("net");
const { URL } = require("url");

const DEFAULT_TTL_SECONDS = 60;
const MIN_TTL_SECONDS = 1;
const MAX_TTL_SECONDS = 86400;
const CACHE_PREFIX = process.env.CACHE_KEY_PREFIX || "agritrust";

const stats = { hits: 0, misses: 0, sets: 0, errors: 0 };

function parseTtlSeconds(value = process.env.CACHE_TTL_SECONDS) {
  if (value === undefined || value === null || value === "") return DEFAULT_TTL_SECONDS;
  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed < MIN_TTL_SECONDS || parsed > MAX_TTL_SECONDS) {
    throw new Error(`CACHE_TTL_SECONDS must be an integer between ${MIN_TTL_SECONDS} and ${MAX_TTL_SECONDS}`);
  }
  return parsed;
}

function isCacheEnabled() { return process.env.CACHE_ENABLED !== "false"; }
function buildKey(namespace, id) { return `${CACHE_PREFIX}:${namespace}:${id}`; }

class MemoryCacheStore {
  constructor(now = () => Date.now()) { this.entries = new Map(); this.now = now; }
  async get(key) {
    const entry = this.entries.get(key);
    if (!entry) return null;
    if (entry.expiresAt <= this.now()) { this.entries.delete(key); return null; }
    return entry.value;
  }
  async set(key, value, ttlSeconds) { this.entries.set(key, { value, expiresAt: this.now() + ttlSeconds * 1000 }); }
  async del(key) { this.entries.delete(key); }
  async clear() { this.entries.clear(); }
}

function encodeCommand(parts) {
  return `*${parts.length}\r\n${parts.map((part) => {
    const value = String(part);
    return `$${Buffer.byteLength(value)}\r\n${value}\r\n`;
  }).join("")}`;
}

function parseRedisReply(buffer) {
  const text = buffer.toString("utf8");
  const type = text[0];
  if (type === "-") throw new Error(text.slice(1).split("\r\n")[0]);
  if (type === "+") return text.slice(1).split("\r\n")[0];
  if (type === ":") return Number(text.slice(1).split("\r\n")[0]);
  if (type === "$") {
    const [lengthLine] = text.slice(1).split("\r\n");
    const length = Number(lengthLine);
    if (length === -1) return null;
    const start = lengthLine.length + 3;
    return text.slice(start, start + length);
  }
  throw new Error("Unsupported Redis response");
}

class RedisCacheStore {
  constructor(redisUrl, timeoutMs = 100) {
    const parsed = new URL(redisUrl);
    this.host = parsed.hostname;
    this.port = Number(parsed.port || 6379);
    this.password = parsed.password ? decodeURIComponent(parsed.password) : undefined;
    this.db = parsed.pathname && parsed.pathname !== "/" ? parsed.pathname.slice(1) : undefined;
    this.timeoutMs = timeoutMs;
  }

  async command(parts) {
    const setup = [];
    if (this.password) setup.push(["AUTH", this.password]);
    if (this.db) setup.push(["SELECT", this.db]);
    const commands = [...setup, parts];

    return new Promise((resolve, reject) => {
      const socket = net.createConnection({ host: this.host, port: this.port });
      let data = Buffer.alloc(0);
      const timer = setTimeout(() => {
        socket.destroy();
        reject(new Error("Redis command timed out"));
      }, this.timeoutMs);
      socket.on("connect", () => socket.write(commands.map(encodeCommand).join("")));
      socket.on("data", (chunk) => { data = Buffer.concat([data, chunk]); });
      socket.on("error", reject);
      socket.on("close", () => {
        clearTimeout(timer);
        try {
          // For setup commands, the final reply is sufficient for API use.
          const replies = data.toString("utf8").split(/(?=[+\-$:])/).filter(Boolean);
          resolve(parseRedisReply(Buffer.from(replies[replies.length - 1] || "")));
        } catch (err) { reject(err); }
      });
      socket.setTimeout(this.timeoutMs, () => socket.end());
    });
  }

  async get(key) { return this.command(["GET", key]); }
  async set(key, value, ttlSeconds) { await this.command(["SET", key, value, "EX", ttlSeconds]); }
  async del(key) { await this.command(["DEL", key]); }
}

let store = new MemoryCacheStore();

async function configureCacheStore() {
  store = process.env.REDIS_URL ? new RedisCacheStore(process.env.REDIS_URL) : new MemoryCacheStore();
  return store;
}

async function getOrSet(key, fetcher, options = {}) {
  if (!isCacheEnabled()) return fetcher();
  const ttlSeconds = options.ttlSeconds || parseTtlSeconds();
  try {
    const cached = await store.get(key);
    if (cached !== null && cached !== undefined) { stats.hits += 1; return JSON.parse(cached); }
    stats.misses += 1;
  } catch (_err) { stats.errors += 1; }
  const value = await fetcher();
  try { await store.set(key, JSON.stringify(value), ttlSeconds); stats.sets += 1; } catch (_err) { stats.errors += 1; }
  return value;
}

function getCacheStats() { return { ...stats, enabled: isCacheEnabled(), ttl_seconds: parseTtlSeconds() }; }
function resetCacheForTests(nextStore = new MemoryCacheStore()) {
  store = nextStore;
  stats.hits = 0; stats.misses = 0; stats.sets = 0; stats.errors = 0;
}

module.exports = { DEFAULT_TTL_SECONDS, MemoryCacheStore, RedisCacheStore, buildKey, configureCacheStore, getCacheStats, getOrSet, parseTtlSeconds, resetCacheForTests };
