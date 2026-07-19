# Capacity Planning with Historical Usage Trending

AgriTrust records per-service request counts, error counts, and latency samples in bounded in-memory time buckets. The capacity planner converts those historical buckets into rolling average RPM, peak RPM, P99 latency, next-hour demand forecasts, and recommended per-minute capacity.

## Architecture

- `createCapacityRecorder` is installed as system-wide Express middleware so every service route contributes usage history.
- `CapacityPlanner` keeps a bounded rolling window and prunes stale buckets on writes and reads.
- `/health/capacity` returns JSON trends for operators and canary analysis.
- `/health/metrics` emits Prometheus gauges for dashboard panels and alert rules.

## SLOs and alerting

Critical paths target P99 latency under 100 ms. Capacity alerts are emitted when peak utilization reaches 80% of configured capacity or when observed P99 latency exceeds the target.

Recommended alert rules:

```promql
agritrust_capacity_utilization >= 0.8
agritrust_capacity_p99_latency_ms > 100
```

## Blue-green and canary rollout

During deploys, compare `/health/capacity` for blue and green pools. Continue rollout only when green P99 latency remains below 100 ms and capacity utilization stays below 80% for the canary window.

## Runbook

1. Inspect `/health/capacity` for services with `capacity_utilization_high` or `p99_latency_target_breached` alerts.
2. Raise service replicas, worker concurrency, or pool limits to at least `recommended_capacity_per_minute`.
3. Verify Prometheus metrics and application logs after scaling.
4. If utilization remains high during canary, halt rollout and route traffic back to blue.
