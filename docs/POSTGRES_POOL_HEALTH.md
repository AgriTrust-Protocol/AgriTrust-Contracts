# PostgreSQL Connection Pool Health Probe with Adaptive Sizing

## Architecture

AgriTrust services expose a system-wide `/health/postgres` probe backed by `AdaptivePostgresPool`. The probe acquires a pooled PostgreSQL client, executes `SELECT 1`, records latency, returns `200` only when the pool and query succeed, and returns `503` for disabled or degraded states. `/health/metrics` exports Prometheus-compatible counters and gauges for alerting and dashboards.

## Adaptive Sizing

The pool starts at `PG_POOL_MAX` and is bounded by `PG_POOL_FLOOR` and `PG_POOL_CEILING`. Each successful probe compares pool utilization, waiter count, and probe latency against `PG_POOL_TARGET_P99_MS` (default `100`). Saturation, waiters, or latency at/above target scale the pool up by `PG_POOL_SCALE_STEP`; sustained low utilization scales it down without dropping below the floor.

## Monitoring and Alerts

Track these metrics:

- `agritrust_postgres_pool_max`
- `agritrust_postgres_pool_probes_total`
- `agritrust_postgres_pool_probe_failures_total`
- `agritrust_postgres_pool_resizes_total`
- `agritrust_postgres_pool_last_latency_ms`

Recommended alerts:

- Page if `/health/postgres` returns `503` for two consecutive checks.
- Page if probe P99 is >= 100ms for five minutes.
- Warn if resizes happen more than ten times in ten minutes, which indicates unstable demand or an undersized floor.

## Deployment

Deploy with blue-green or canary rollout by routing 5% of traffic to the new service, confirming `/health/postgres` success rate is 99.99% and probe P99 is under 100ms, then increasing traffic in 25% increments. Roll back if health failures exceed the alert threshold or pool resize churn spikes.

## Security Notes

The health endpoint never returns credentials, SQL text beyond the fixed `SELECT 1` probe, or stack traces. Configure PostgreSQL credentials only through `DATABASE_URL` secrets management.
