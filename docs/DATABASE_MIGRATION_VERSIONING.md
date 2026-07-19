# Database Migration Versioning with Rollback Support

AgriTrust services use `DatabaseMigrationManager` to apply and roll back PostgreSQL schema migrations with explicit versions, immutable checksums, and operational metrics. The manager is intentionally small and framework-neutral so every service can share the same safety model while keeping request critical paths under the 100ms P99 target.

## Architecture

- Each migration declares a positive integer `version`, a human-readable `name`, an `up(db)` function, and a `down(db)` function.
- The manager normalizes the catalog, rejects duplicate versions, computes a SHA-256 checksum from the migration content, and stores applied versions in `schema_migrations`.
- Apply and rollback operations run behind `pg_advisory_lock` to prevent concurrent deploy jobs from racing.
- Each migration runs inside a database transaction with `SET LOCAL statement_timeout` defaulting to 100ms. Increase this per deployment only after review for large backfills.
- Rollback moves from the current version down to the requested target in reverse order and marks rows with `rolled_back_at` for auditability.

## Monitoring and Alerting

Expose `prometheusMetrics()` from the migration job or admin service:

- `agritrust_db_migration_current_version`
- `agritrust_db_migrations_applied_total`
- `agritrust_db_migrations_rolled_back_total`
- `agritrust_db_migrations_failed_total`
- `agritrust_db_migration_last_duration_ms`

Alert when failures are non-zero during a deployment window or when `last_duration_ms` approaches the statement-timeout budget. Dashboards should graph current version by service and compare blue versus green pools before promotion.

## Blue-Green and Canary Deployment

1. Run migrations against the green database target while blue continues serving production traffic.
2. Route 5% of canary traffic to green after the migration job reports the expected version and zero failures.
3. Compare canary error rate, migration duration, and PostgreSQL pool health for at least one analysis window.
4. Promote in 25% increments only while uptime remains at or above 99.99% and critical paths remain below 100ms P99.
5. Roll back traffic first, then call `rollback(targetVersion)` if schema changes must be reversed.

## Security Review Checklist

- Verify each `down(db)` path is tested and does not drop data unless the security review explicitly approves it.
- Confirm migrations do not log database credentials, tenant data, grant details, or raw SQL parameters containing sensitive values.
- Confirm long-running data backfills are chunked outside request paths and are guarded by legal-hold and tenant boundaries where applicable.
