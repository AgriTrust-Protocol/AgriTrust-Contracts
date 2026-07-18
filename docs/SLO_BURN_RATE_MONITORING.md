# Service Level Objective Monitoring and Burn Rate Alerts

AgriTrust services share one SLO policy for externally visible critical paths:

- **Latency:** P99 request latency must stay below **100 ms**.
- **Availability:** successful responses must meet or exceed **99.99%**.
- **Security:** alert routes, dashboards, and metric ingestion are restricted to the production observability tenant and reviewed with each service security checklist.

## Architecture

Each service emits request counters, error counters, and latency histograms to the observability plane. `src/services/sloMonitor.js` contains the reference implementation used by tests and service adapters to calculate windowed service-level indicators, availability error-budget burn rate, latency objective status, and Prometheus-compatible metrics.

```text
Service telemetry -> SLO evaluator -> Prometheus metrics -> Alertmanager -> on-call / ticket queue
                                      -> Grafana dashboard -> release SLO gates
```

The evaluator compares a short window with a long window so transient spikes do not page unless they are also consuming the error budget at a sustained rate. Latency pages require the P99 latency target to be breached in both windows.

## Metrics

The SLO evaluator exports these metric names:

| Metric | Meaning |
| --- | --- |
| `agritrust_slo_availability_ratio` | Short-window availability ratio. |
| `agritrust_slo_p99_latency_ms` | Short-window critical-path P99 latency. |
| `agritrust_slo_burn_rate_short` | Short-window error-budget burn multiple. |
| `agritrust_slo_burn_rate_long` | Long-window error-budget burn multiple. |
| `agritrust_slo_alert_state` | `0` for healthy, `1` for ticket, `2` for page. |

Dashboard rows should show current availability, P99 latency, short/long burn rates, request volume, error volume, and the active alert reason grouped by `service`, `route`, `environment`, and `release_color` labels.

## Alert policy

| Alert | Condition | Response |
| --- | --- | --- |
| Page burn | Short and long windows are both burning at `14.4x` or more. | Page primary on-call and start the SLO incident runbook. |
| Ticket burn | Short and long windows are both burning at `6x` or more. | Create a ticket for same-business-day triage. |
| Critical latency | Short and long window P99 latency are both `>= 100 ms`. | Page owning team and inspect dependency saturation before scaling. |

## Blue-green and canary gates

1. Deploy the SLO evaluator, dashboards, and alerts to the green environment without shifting user traffic.
2. Mirror telemetry from blue and green for at least one long window and confirm alert parity.
3. Shift 5% of traffic to green and compare availability, P99 latency, and burn rate by `release_color`.
4. Increase to 25%, 50%, and 100% only when green stays below ticket burn and below 100 ms P99 for two consecutive long windows.
5. Roll back immediately if a page burn or critical latency alert fires during canary analysis.

## Security review checklist

- Metric endpoints are internal-only or protected by service mesh authorization.
- Dashboard access is least-privilege and audited.
- Alert webhooks use managed secrets and rotation policies.
- Labels do not include personally identifiable information, secrets, wallet private keys, or raw bearer tokens.
- Runbook evidence includes dashboard snapshots, alert IDs, and deployment identifiers for auditability.
