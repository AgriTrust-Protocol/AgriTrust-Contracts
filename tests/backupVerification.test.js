"use strict";

const {
  buildTimestamp,
  createVerifier,
  redact,
  validateRestoreDatabaseName,
} = require("../src/services/backupVerification");

describe("backup verification service", () => {
  it("formats timestamps for portable backup filenames", () => {
    expect(buildTimestamp(new Date("2026-07-17T12:34:56.789Z"))).toBe("2026-07-17T12-34-56-789Z");
  });

  it("redacts database URLs in verification output", () => {
    expect(redact("postgres://user:password@database.internal/agritrust")).toBe("post…rust");
  });

  it("rejects unsafe restore database names", () => {
    expect(() => validateRestoreDatabaseName("restore_db-1")).not.toThrow();
    expect(() => validateRestoreDatabaseName("restore;DROP DATABASE prod")).toThrow(/letters/);
  });

  it("runs dump, restore, integrity, and retention steps in order", async () => {
    const calls = [];
    const runCommand = jest.fn(async (command, args, options) => {
      calls.push({ command, args, env: options.env });
      return { command, args, stdout: "", stderr: "", durationMs: 7 };
    });

    const verifier = createVerifier({
      env: {
        DATABASE_URL: "postgres://primary/agritrust",
        RESTORE_DATABASE_URL: "postgres://restore/postgres",
        RESTORE_DATABASE_NAME: "restore_verify",
        BACKUP_DIR: "/tmp/backups",
        BACKUP_RETENTION_DAYS: "14",
      },
      now: () => new Date("2026-07-17T00:00:00.000Z"),
      runCommand,
    });

    const result = await verifier.verify();

    expect(result).toMatchObject({
      backupPath: "/tmp/backups/agritrust-2026-07-17T00-00-00-000Z.dump",
      restoreDatabaseName: "restore_verify",
      status: "passed",
    });
    expect(result.primaryDatabaseUrl).toBe("post…rust");
    expect(result.restoreDatabaseUrl).toBe("post…gres");
    expect(calls.map(call => call.command)).toEqual([
      "pg_dump",
      "dropdb",
      "createdb",
      "pg_restore",
      "psql",
      "find",
    ]);
    expect(calls[0].args).toContain("--format=custom");
    expect(calls[3].args).toContain("--dbname");
    expect(calls[5].args).toEqual(["/tmp/backups", "-name", "agritrust-*.dump", "-mtime", "+14", "-delete"]);
  });

  it("requires primary and restore database URLs", async () => {
    const verifier = createVerifier({ env: {}, runCommand: jest.fn() });
    await expect(verifier.verify()).rejects.toThrow("DATABASE_URL");
  });
});
