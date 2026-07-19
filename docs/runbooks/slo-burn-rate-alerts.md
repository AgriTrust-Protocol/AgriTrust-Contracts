# SLO Burn Rate Alert Runbook

## Triage

1. Identify the firing alert: page burn, ticket burn, or critical latency.
2. Open the SLO dashboard filtered by `service`, `route`, `environment`, and `release_color`.
3. Compare the short-window and long-window burn rates with request volume to rule out low-traffic noise.
4. Check dependency dashboards for PostgreSQL pool health, Kafka consumer lag, webhook delivery failures, and service mesh mTLS errors.

## Mitigation

- If the alert started after a deployment, pause canary progression and route traffic back to blue.
- If P99 latency is `>= 100 ms`, scale the saturated service or dependency and disable optional non-critical work.
- If availability is below `99.99%`, prioritize reducing 5xx responses and failed dependency calls before feature work.
- If the burn rate is caused by a security control or suspicious traffic, involve security review before suppressing alerts.

## Resolution

1. Confirm `agritrust_slo_alert_state` returns to `0` for two consecutive long windows.
2. Verify P99 latency is below `100 ms` and availability is at least `99.99%`.
3. Attach dashboard snapshots, deployment IDs, and mitigation notes to the incident record.
4. Create follow-up tasks for missing dashboards, alert tuning, or tests found during the incident.
