# Incident Response Runbook Automation with PagerDuty

## Architecture

AgriTrust exposes a small incident automation surface under `/incidents` that maps platform signals to approved runbooks and PagerDuty Events API v2 payloads. The core service is intentionally synchronous and in-memory so critical-path automation stays below the 100ms P99 target when using dry-run or an injected queueing client.

```text
monitor / alert rule -> /incidents/trigger -> runbook registry -> PagerDuty Events API -> responder
                                      |-> /incidents/metrics
                                      |-> /incidents/runbooks
```

## Runbook registry

The built-in registry covers the system-wide services that already have operational documentation:

| Runbook id | Component | Linked documentation | Dashboard |
| --- | --- | --- | --- |
| `postgres_pool_exhaustion` | Postgres | `/docs/runbooks/postgres-pool-health.md` | `dashboards/postgres-pool-health` |
| `webhook_delivery_degradation` | Webhooks | `/docs/runbooks/webhook-delivery.md` | `dashboards/webhook-delivery` |
| `service_mesh_mtls_failure` | Service mesh | `/docs/runbooks/service-mesh-mtls.md` | `dashboards/service-mesh-mtls` |

## PagerDuty integration

Set `PAGERDUTY_ROUTING_KEY` in the deployment environment. The service sends Events API v2 `trigger` events with deterministic dedup keys, severity, runbook links, dashboards, and first-response steps. Set `PAGERDUTY_DRY_RUN=true` for blue-green preflight and canary validation without paging responders.

Example canary request:

```bash
curl -X POST 'https://api.example/incidents/trigger?dryRun=true' \
  -H 'content-type: application/json' \
  -d '{"runbookId":"service_mesh_mtls_failure","severity":"critical","incidentKey":"canary-mesh"}'
```

## Monitoring and alerting

`GET /incidents/metrics` returns trigger counts, PagerDuty event counts, failure counts, average latency, and P99 latency. Alert when `failures > 0` over five minutes or when `p99LatencyMs >= 100`.

## Deployment plan

1. Deploy the green version with `PAGERDUTY_DRY_RUN=true`.
2. Send one dry-run canary for each runbook id and verify metrics increment.
3. Shift 5% traffic to green and watch `/incidents/metrics`, API latency, and error rates.
4. Remove dry-run only after security review confirms routing-key storage and responder policy.
5. Shift to 100% green; keep blue warm for rollback until canary analysis passes.
