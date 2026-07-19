"use strict";

const crypto = require("crypto");

const DEFAULTS = Object.freeze({
  lockKey: 830083,
  statementTimeoutMs: Number(process.env.DB_MIGRATION_STATEMENT_TIMEOUT_MS || 100),
  tableName: "schema_migrations",
});

function checksumOf(migration) {
  const payload = [migration.version, migration.name, migration.up.toString(), migration.down.toString()].join("\n---\n");
  return crypto.createHash("sha256").update(payload).digest("hex");
}

function normalizeMigration(migration) {
  if (!migration || !Number.isInteger(migration.version) || migration.version <= 0) {
    throw new Error("migration version must be a positive integer");
  }
  if (!migration.name || typeof migration.up !== "function" || typeof migration.down !== "function") {
    throw new Error(`migration ${migration.version} must define name, up, and down`);
  }
  return { ...migration, checksum: migration.checksum || checksumOf(migration) };
}

function normalizeCatalog(migrations) {
  const normalized = migrations.map(normalizeMigration).sort((a, b) => a.version - b.version);
  const seen = new Set();
  for (const migration of normalized) {
    if (seen.has(migration.version)) {
      throw new Error(`duplicate migration version ${migration.version}`);
    }
    seen.add(migration.version);
  }
  return normalized;
}

class DatabaseMigrationManager {
  constructor({ db, migrations = [], logger = console, config = {} }) {
    if (!db || typeof db.query !== "function") {
      throw new Error("db query client is required");
    }
    this.db = db;
    this.logger = logger;
    this.config = { ...DEFAULTS, ...config };
    this.migrations = normalizeCatalog(migrations);
    this.metrics = {
      applied_total: 0,
      rolled_back_total: 0,
      failed_total: 0,
      last_duration_ms: 0,
      current_version: 0,
    };
  }

  async ensureVersionTable() {
    await this.db.query(`CREATE TABLE IF NOT EXISTS ${this.config.tableName} (
      version INTEGER PRIMARY KEY,
      name TEXT NOT NULL,
      checksum TEXT NOT NULL,
      applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
      rolled_back_at TIMESTAMPTZ
    )`);
  }

  async withLock(operation) {
    await this.db.query("SELECT pg_advisory_lock($1)", [this.config.lockKey]);
    try {
      return await operation();
    } finally {
      await this.db.query("SELECT pg_advisory_unlock($1)", [this.config.lockKey]);
    }
  }

  async appliedVersions() {
    const result = await this.db.query(
      `SELECT version, name, checksum FROM ${this.config.tableName} WHERE rolled_back_at IS NULL ORDER BY version ASC`,
    );
    return result.rows || [];
  }

  async currentVersion() {
    await this.ensureVersionTable();
    const rows = await this.appliedVersions();
    const version = rows.length === 0 ? 0 : rows[rows.length - 1].version;
    this.metrics.current_version = version;
    return version;
  }

  assertChecksums(applied) {
    const catalog = new Map(this.migrations.map((migration) => [migration.version, migration]));
    for (const row of applied) {
      const migration = catalog.get(row.version);
      if (!migration) {
        throw new Error(`applied migration ${row.version} is missing from catalog`);
      }
      if (migration.checksum !== row.checksum) {
        throw new Error(`checksum mismatch for migration ${row.version}`);
      }
    }
  }

  async migrate(targetVersion = null) {
    const started = Date.now();
    return this.withLock(async () => {
      await this.ensureVersionTable();
      const applied = await this.appliedVersions();
      this.assertChecksums(applied);
      const current = applied.length === 0 ? 0 : applied[applied.length - 1].version;
      const ceiling = targetVersion == null ? this.migrations.at(-1)?.version || 0 : targetVersion;
      const pending = this.migrations.filter((migration) => migration.version > current && migration.version <= ceiling);
      for (const migration of pending) {
        await this.applyMigration(migration);
      }
      this.metrics.current_version = pending.at(-1)?.version || current;
      this.metrics.last_duration_ms = Date.now() - started;
      return { from: current, to: this.metrics.current_version, applied: pending.map((migration) => migration.version) };
    });
  }

  async rollback(targetVersion) {
    if (!Number.isInteger(targetVersion) || targetVersion < 0) {
      throw new Error("rollback targetVersion must be a non-negative integer");
    }
    const started = Date.now();
    return this.withLock(async () => {
      await this.ensureVersionTable();
      const applied = await this.appliedVersions();
      this.assertChecksums(applied);
      const current = applied.length === 0 ? 0 : applied[applied.length - 1].version;
      const catalog = new Map(this.migrations.map((migration) => [migration.version, migration]));
      const toRollback = applied.filter((row) => row.version > targetVersion).sort((a, b) => b.version - a.version);
      for (const row of toRollback) {
        await this.rollbackMigration(catalog.get(row.version));
      }
      this.metrics.current_version = targetVersion;
      this.metrics.last_duration_ms = Date.now() - started;
      return { from: current, to: targetVersion, rolledBack: toRollback.map((row) => row.version) };
    });
  }

  async applyMigration(migration) {
    await this.db.query("BEGIN");
    try {
      await this.db.query("SET LOCAL statement_timeout = $1", [this.config.statementTimeoutMs]);
      await migration.up(this.db);
      await this.db.query(
        `INSERT INTO ${this.config.tableName} (version, name, checksum) VALUES ($1, $2, $3)`,
        [migration.version, migration.name, migration.checksum],
      );
      await this.db.query("COMMIT");
      this.metrics.applied_total += 1;
    } catch (err) {
      await this.db.query("ROLLBACK");
      this.metrics.failed_total += 1;
      this.logger.error("migration apply failed", { version: migration.version, error: err.message });
      throw err;
    }
  }

  async rollbackMigration(migration) {
    await this.db.query("BEGIN");
    try {
      await this.db.query("SET LOCAL statement_timeout = $1", [this.config.statementTimeoutMs]);
      await migration.down(this.db);
      await this.db.query(`UPDATE ${this.config.tableName} SET rolled_back_at = NOW() WHERE version = $1`, [migration.version]);
      await this.db.query("COMMIT");
      this.metrics.rolled_back_total += 1;
    } catch (err) {
      await this.db.query("ROLLBACK");
      this.metrics.failed_total += 1;
      this.logger.error("migration rollback failed", { version: migration.version, error: err.message });
      throw err;
    }
  }

  prometheusMetrics() {
    return [
      `agritrust_db_migration_current_version ${this.metrics.current_version}`,
      `agritrust_db_migrations_applied_total ${this.metrics.applied_total}`,
      `agritrust_db_migrations_rolled_back_total ${this.metrics.rolled_back_total}`,
      `agritrust_db_migrations_failed_total ${this.metrics.failed_total}`,
      `agritrust_db_migration_last_duration_ms ${this.metrics.last_duration_ms}`,
    ].join("\n");
  }
}

module.exports = { DatabaseMigrationManager, checksumOf, normalizeCatalog, DEFAULTS };
