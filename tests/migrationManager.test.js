"use strict";

const { DatabaseMigrationManager, normalizeCatalog } = require("../src/services/migrationManager");

class FakeDb {
  constructor() {
    this.rows = [];
    this.calls = [];
  }

  async query(sql, params = []) {
    this.calls.push({ sql, params });
    if (sql.startsWith("SELECT version")) {
      return { rows: this.rows.filter((row) => !row.rolled_back_at).sort((a, b) => a.version - b.version) };
    }
    if (sql.startsWith("INSERT INTO")) {
      this.rows.push({ version: params[0], name: params[1], checksum: params[2] });
    }
    if (sql.startsWith("UPDATE")) {
      const row = this.rows.find((item) => item.version === params[0] && !item.rolled_back_at);
      row.rolled_back_at = new Date();
    }
    return { rows: [] };
  }
}

function migrations(events) {
  return [
    {
      version: 1,
      name: "create_grants",
      up: async () => events.push("up:1"),
      down: async () => events.push("down:1"),
    },
    {
      version: 2,
      name: "add_indexes",
      up: async () => events.push("up:2"),
      down: async () => events.push("down:2"),
    },
  ];
}

describe("DatabaseMigrationManager", () => {
  test("applies pending migrations in version order under an advisory lock", async () => {
    const db = new FakeDb();
    const events = [];
    const manager = new DatabaseMigrationManager({ db, migrations: migrations(events), logger: { error: jest.fn() } });

    const result = await manager.migrate();

    expect(result).toEqual({ from: 0, to: 2, applied: [1, 2] });
    expect(events).toEqual(["up:1", "up:2"]);
    expect(db.calls[0].sql).toBe("SELECT pg_advisory_lock($1)");
    expect(db.calls.at(-1).sql).toBe("SELECT pg_advisory_unlock($1)");
    expect(manager.prometheusMetrics()).toContain("agritrust_db_migration_current_version 2");
  });

  test("rolls back migrations in reverse order to requested version", async () => {
    const db = new FakeDb();
    const events = [];
    const manager = new DatabaseMigrationManager({ db, migrations: migrations(events), logger: { error: jest.fn() } });

    await manager.migrate();
    const result = await manager.rollback(0);

    expect(result).toEqual({ from: 2, to: 0, rolledBack: [2, 1] });
    expect(events).toEqual(["up:1", "up:2", "down:2", "down:1"]);
    expect(manager.metrics.rolled_back_total).toBe(2);
  });

  test("rejects duplicate versions and checksum drift", async () => {
    expect(() => normalizeCatalog([{ version: 1, name: "a", up() {}, down() {} }, { version: 1, name: "b", up() {}, down() {} }])).toThrow("duplicate");

    const db = new FakeDb();
    const events = [];
    const manager = new DatabaseMigrationManager({ db, migrations: migrations(events), logger: { error: jest.fn() } });
    await manager.migrate(1);
    db.rows[0].checksum = "tampered";

    await expect(manager.migrate()).rejects.toThrow("checksum mismatch");
  });

  test("rolls back transaction and increments failure metric when migration fails", async () => {
    const db = new FakeDb();
    const logger = { error: jest.fn() };
    const manager = new DatabaseMigrationManager({
      db,
      logger,
      migrations: [{ version: 1, name: "bad", up: async () => { throw new Error("boom"); }, down: async () => {} }],
    });

    await expect(manager.migrate()).rejects.toThrow("boom");

    expect(db.calls.some((call) => call.sql === "ROLLBACK")).toBe(true);
    expect(manager.metrics.failed_total).toBe(1);
    expect(logger.error).toHaveBeenCalledWith("migration apply failed", { version: 1, error: "boom" });
  });
});
