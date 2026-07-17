# Scheduled Database Backup Verification with Restore Testing

## Architecture

The backup verification workflow proves that production backups are usable by restoring every scheduled dump into an isolated verification database before a backup is considered healthy. The process is intentionally out-of-band from user request paths, so escrow and contract API critical paths remain unaffected and can continue targeting a sub-100ms P99 latency budget.

1. A scheduler invokes `npm run backup:verify` from a hardened worker or Kubernetes CronJob.
2. The worker runs `pg_dump` against `DATABASE_URL` using a custom-format dump.
3. The worker recreates `RESTORE_DATABASE_NAME` on the isolated restore host referenced by `RESTORE_DATABASE_URL`.
4. `pg_restore` loads the dump into the restore database.
5. `psql` executes a minimal integrity probe (`SELECT 1`) and emits structured JSON with the verification ID, durations, and redacted connection strings.
6. The worker removes backup artifacts older than `BACKUP_RETENTION_DAYS`.

## Operational settings

| Variable | Required | Default | Purpose |
| --- | --- | --- | --- |
| `DATABASE_URL` | Yes | None | Primary database connection string used by `pg_dump`. |
| `RESTORE_DATABASE_URL` | Yes | None | Isolated restore database connection string used by `pg_restore` and `psql`. |
| `RESTORE_DATABASE_NAME` | No | `agritrust_restore_verify` | Database recreated for restore checks. Only letters, numbers, `_`, and `-` are accepted. |
| `BACKUP_DIR` | No | `/var/backups/agritrust` | Directory where custom-format dumps are written. |
| `BACKUP_RETENTION_DAYS` | No | `30` | Local dump retention window. |
| `BACKUP_VERIFY_TIMEOUT_MS` | No | `1800000` | Per-command timeout. |

## Monitoring and alerting

Emit the script JSON output to the platform log pipeline and derive these service-level indicators:

- `backup_verification_status{status="passed|failed"}` from the command exit code.
- `backup_verification_duration_ms` from the top-level `durationMs` field.
- `backup_verification_step_duration_ms{step}` from each step in `steps`.
- `backup_verification_age_seconds` from the most recent successful verification timestamp.

Recommended alerts:

- Page if no successful verification has completed in 26 hours.
- Page if two consecutive verification attempts fail.
- Warn if total verification duration exceeds 80% of the scheduler interval.
- Warn if restore or integrity-check step duration changes by more than 3x from the seven-day baseline.

## Deployment plan

Use a blue-green rollout for the scheduler configuration:

1. Deploy the verifier image and secrets to the green environment with the schedule disabled.
2. Run a manual verification and compare runtime, logs, and database load against blue.
3. Enable a canary schedule in green at 10% of the normal cadence for one day.
4. Promote green to the full schedule after one successful canary window and no database saturation alerts.
5. Disable blue only after green has produced at least one verified backup artifact and a successful restore.

## Security review checklist

- `DATABASE_URL` and `RESTORE_DATABASE_URL` are supplied only through the secret manager.
- Restore databases are isolated from production and have least-privilege credentials.
- Logs contain redacted connection strings and no dump contents.
- Backup artifacts are encrypted by the storage layer and have lifecycle retention.
- Restore database names are validated before shelling out to PostgreSQL utilities.

## Runbook

### Manual verification

```bash
DATABASE_URL=postgres://primary/agritrust \
RESTORE_DATABASE_URL=postgres://restore/postgres \
RESTORE_DATABASE_NAME=agritrust_restore_verify \
npm run backup:verify
```

### Failure response

1. Confirm the failing step from the structured JSON output.
2. For `backup` failures, verify primary database connectivity and free space in `BACKUP_DIR`.
3. For `restore` failures, preserve the dump file, recreate the restore host, and rerun the command manually with the same dump.
4. For `integrity-check` failures, quarantine the backup artifact and open an incident because the backup cannot be trusted.
5. Keep the previous known-good backup until a new restore verification passes.
