"use strict";

const { spawn } = require("child_process");
const crypto = require("crypto");

const DEFAULT_TIMEOUT_MS = 30 * 60 * 1000;
const RESTORE_DATABASE_PATTERN = /^[a-zA-Z0-9_\-]+$/;

function buildTimestamp(date = new Date()) {
  return date.toISOString().replace(/[:.]/g, "-");
}

function requireEnv(name, env = process.env) {
  const value = env[name];
  if (!value || value.trim() === "") {
    throw new Error(`Missing required environment variable: ${name}`);
  }
  return value;
}

function validateRestoreDatabaseName(databaseName) {
  if (!RESTORE_DATABASE_PATTERN.test(databaseName)) {
    throw new Error("Restore database name may only contain letters, numbers, underscores, and dashes");
  }
  return databaseName;
}

function redact(value) {
  if (!value) return value;
  if (value.length <= 8) return "********";
  return `${value.slice(0, 4)}…${value.slice(-4)}`;
}

function runCommand(command, args, options = {}) {
  const timeoutMs = options.timeoutMs || DEFAULT_TIMEOUT_MS;
  const env = options.env || process.env;

  return new Promise((resolve, reject) => {
    const startedAt = Date.now();
    const child = spawn(command, args, {
      env,
      stdio: ["ignore", "pipe", "pipe"],
    });

    let stdout = "";
    let stderr = "";
    let timedOut = false;

    const timeout = setTimeout(() => {
      timedOut = true;
      child.kill("SIGTERM");
    }, timeoutMs);

    child.stdout.on("data", chunk => {
      stdout += chunk.toString();
    });

    child.stderr.on("data", chunk => {
      stderr += chunk.toString();
    });

    child.on("error", error => {
      clearTimeout(timeout);
      reject(error);
    });

    child.on("close", code => {
      clearTimeout(timeout);
      const durationMs = Date.now() - startedAt;
      if (timedOut) {
        reject(new Error(`${command} timed out after ${timeoutMs}ms`));
        return;
      }
      if (code !== 0) {
        const error = new Error(`${command} exited with code ${code}: ${stderr.trim()}`);
        error.code = code;
        error.stdout = stdout;
        error.stderr = stderr;
        reject(error);
        return;
      }
      resolve({ command, args, stdout, stderr, durationMs });
    });
  });
}

function createVerifier(config = {}) {
  const env = config.env || process.env;
  const run = config.runCommand || runCommand;
  const now = config.now || (() => new Date());
  const timeoutMs = Number(config.timeoutMs || env.BACKUP_VERIFY_TIMEOUT_MS || DEFAULT_TIMEOUT_MS);
  const retentionDays = Number(config.retentionDays || env.BACKUP_RETENTION_DAYS || 30);
  const backupDir = config.backupDir || env.BACKUP_DIR || "/var/backups/agritrust";
  const primaryUrl = config.primaryDatabaseUrl || env.DATABASE_URL;
  const restoreUrl = config.restoreDatabaseUrl || env.RESTORE_DATABASE_URL;
  const restoreDatabaseName = validateRestoreDatabaseName(
    config.restoreDatabaseName || env.RESTORE_DATABASE_NAME || "agritrust_restore_verify"
  );

  async function verify() {
    if (!primaryUrl) requireEnv("DATABASE_URL", env);
    if (!restoreUrl) requireEnv("RESTORE_DATABASE_URL", env);

    const timestamp = buildTimestamp(now());
    const backupPath = `${backupDir}/agritrust-${timestamp}.dump`;
    const verificationId = crypto.createHash("sha256").update(`${backupPath}:${restoreDatabaseName}`).digest("hex").slice(0, 16);
    const startedAt = Date.now();
    const steps = [];

    const execute = async (name, command, args, commandEnv = env) => {
      const result = await run(command, args, { env: commandEnv, timeoutMs });
      steps.push({ name, command, durationMs: result.durationMs || 0 });
      return result;
    };

    await execute("backup", "pg_dump", ["--format=custom", "--no-owner", "--file", backupPath, primaryUrl]);
    await execute("drop-restore-db", "dropdb", ["--if-exists", restoreDatabaseName], { ...env, DATABASE_URL: restoreUrl });
    await execute("create-restore-db", "createdb", [restoreDatabaseName], { ...env, DATABASE_URL: restoreUrl });
    await execute("restore", "pg_restore", ["--clean", "--if-exists", "--no-owner", "--dbname", restoreUrl, backupPath]);
    await execute("integrity-check", "psql", [restoreUrl, "--tuples-only", "--no-align", "--command", "SELECT 1"]);
    await execute("cleanup-old-backups", "find", [backupDir, "-name", "agritrust-*.dump", "-mtime", `+${retentionDays}`, "-delete"]);

    return {
      verificationId,
      backupPath,
      restoreDatabaseName,
      primaryDatabaseUrl: redact(primaryUrl),
      restoreDatabaseUrl: redact(restoreUrl),
      durationMs: Date.now() - startedAt,
      steps,
      status: "passed",
    };
  }

  return { verify };
}

module.exports = {
  DEFAULT_TIMEOUT_MS,
  buildTimestamp,
  createVerifier,
  redact,
  runCommand,
  validateRestoreDatabaseName,
};
