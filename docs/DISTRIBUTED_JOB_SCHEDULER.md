# Distributed Job Scheduler with Lease-based Worker Claiming

AgriTrust services use the job scheduler for at-least-once background work that must be claimed safely by horizontally scaled workers. The API exposes enqueue, claim, renew, complete, and metrics endpoints; production deployments should back the same state-machine with a transactional datastore using `SELECT ... FOR UPDATE SKIP LOCKED` or an equivalent compare-and-swap primitive.

## Architecture

- Jobs move through `queued -> leased -> completed` or `failed` states.
- `claimNext` chooses the highest-priority ready job in a queue, assigns a worker id, and returns an opaque lease token that fences all follow-up writes.
- `renewLease` extends ownership only when the caller presents the active worker id and lease token.
- `complete` accepts only the active lease owner and token, preventing stale workers from acknowledging work after a lease expires.
- Expired leases are reclaimed on scheduler operations and retried until `maxAttempts` is exhausted.

## Operations, monitoring, and alerts

`GET /jobs/metrics` returns JSON counters and queue-depth gauges. `GET /jobs/metrics.prom` emits Prometheus text metrics for dashboards and alert rules.

Recommended alerts:

- P99 critical-path latency above 100ms for 5 minutes.
- `lease_conflicts_total` increasing rapidly, which can indicate duplicate workers or clock/configuration issues.
- `expired_leases_reclaimed_total` increasing while `completed_total` is flat, which can indicate worker crashes.
- Failed jobs above the service SLO error budget.

## Deployment runbook

1. Deploy the scheduler API and datastore schema to the green environment.
2. Enable a 5% worker canary against the green endpoint.
3. Compare queue depth, claim latency P99, completion rate, failed jobs, and lease conflicts against blue for at least 30 minutes.
4. Increase canary traffic to 25%, 50%, and 100% if metrics remain within SLOs.
5. Roll back to blue if P99 exceeds 100ms, lease conflicts spike, or completion rate drops materially.

## Security review checklist

- Treat worker ids and queue names as untrusted input; the implementation validates URL-safe identifiers.
- Never log payload secrets or lease tokens.
- Require service-to-service authentication before exposing `/jobs/*` outside the internal mesh.
- Use short lease durations and idempotent job handlers because delivery is at least once.
