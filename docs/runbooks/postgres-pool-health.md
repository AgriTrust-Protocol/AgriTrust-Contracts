# Runbook: PostgreSQL Pool Health

1. Check `/health/postgres` from the affected service instance.
2. Check `/health/metrics` and compare failures, latency, current max size, and resize churn.
3. If degraded, verify `DATABASE_URL` secret rotation and database availability.
4. If latency exceeds 100ms with waiters, raise `PG_POOL_CEILING` or reduce request fan-out.
5. If resize churn is high, increase `PG_POOL_FLOOR` and redeploy through the canary pipeline.
6. Roll back the canary if two consecutive probes return `503` or availability drops below 99.99%.
