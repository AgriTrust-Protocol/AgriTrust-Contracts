"use strict";

const { InMemoryCacheLayer, stableKey } = require("../src/services/cacheLayer");

describe("InMemoryCacheLayer", () => {
  let cache;

  beforeEach(() => {
    cache = new InMemoryCacheLayer({ ttl: 5000, maxSize: 10 });
  });

  afterEach(() => {
    cache.destroy();
  });

  it("stores and retrieves values", () => {
    cache.set("key1", "value1");
    expect(cache.get("key1")).toBe("value1");
  });

  it("returns undefined for missing keys", () => {
    expect(cache.get("nonexistent")).toBeUndefined();
  });

  it("reports has correctly", () => {
    cache.set("key1", "value1");
    expect(cache.has("key1")).toBe(true);
    expect(cache.has("nonexistent")).toBe(false);
  });

  it("deletes values", () => {
    cache.set("key1", "value1");
    cache.delete("key1");
    expect(cache.get("key1")).toBeUndefined();
  });

  it("tracks hit and miss stats", () => {
    cache.get("miss1");
    cache.get("miss2");
    cache.set("hit1", "val");
    cache.get("hit1");

    const stats = cache.stats;
    expect(stats.hits).toBe(1);
    expect(stats.misses).toBe(2);
    expect(stats.hitRate).toBeCloseTo(0.333, 1);
  });

  it("evicts oldest entries by access count when over maxSize", () => {
    for (let i = 0; i < 10; i++) {
      cache.set(`key${i}`, `val${i}`);
    }
    expect(cache.size).toBe(10);

    cache.set("overflow", "val");
    expect(cache.size).toBe(10);
    expect(cache.evictions).toBe(1);
  });

  it("getOrSet caches factory result", () => {
    let calls = 0;
    const factory = () => {
      calls += 1;
      return "computed";
    };

    const first = cache.getOrSet("key", factory);
    const second = cache.getOrSet("key", factory);

    expect(first).toBe("computed");
    expect(second).toBe("computed");
    expect(calls).toBe(1);
  });

  it("getOrSetAsync caches async factory result", async () => {
    let calls = 0;
    const factory = async () => {
      calls += 1;
      return "async_val";
    };

    const first = await cache.getOrSetAsync("key", factory);
    const second = await cache.getOrSetAsync("key", factory);

    expect(first).toBe("async_val");
    expect(second).toBe("async_val");
    expect(calls).toBe(1);
  });

  it("mget returns array of values", () => {
    cache.set("a", 1);
    cache.set("b", 2);
    cache.set("c", 3);
    expect(cache.mget(["a", "b", "c", "missing"])).toEqual([1, 2, 3, undefined]);
  });

  it("mset stores multiple entries", () => {
    cache.mset([["x", 10], ["y", 20], ["z", 30]]);
    expect(cache.get("x")).toBe(10);
    expect(cache.get("y")).toBe(20);
    expect(cache.get("z")).toBe(30);
  });

  it("clear removes all entries and resets stats", () => {
    cache.set("a", 1);
    cache.set("b", 2);
    cache.get("a");
    cache.clear();
    expect(cache.size).toBe(0);
    expect(cache.stats.hits).toBe(0);
    expect(cache.stats.misses).toBe(0);
  });

  it("expires entries after TTL", () => {
    cache = new InMemoryCacheLayer({ ttl: 50 });
    cache.set("ephemeral", "gone");
    expect(cache.get("ephemeral")).toBe("gone");

    return new Promise((resolve) => {
      setTimeout(() => {
        expect(cache.get("ephemeral")).toBeUndefined();
        resolve();
      }, 100);
    });
  });
});

describe("stableKey", () => {
  it("generates consistent keys from parts", () => {
    const k1 = stableKey(["user", 42, { role: "admin", org: "agri" }]);
    const k2 = stableKey(["user", 42, { org: "agri", role: "admin" }]);
    expect(k1).toBe(k2);
  });

  it("generates different keys for different inputs", () => {
    const k1 = stableKey(["a", 1]);
    const k2 = stableKey(["a", 2]);
    expect(k1).not.toBe(k2);
  });
});
