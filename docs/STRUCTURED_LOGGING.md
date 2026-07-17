# Structured Logging with OpenTelemetry Semantic Conventions

The API emits one JSON log record per HTTP request plus structured lifecycle and
error records. Records use OpenTelemetry-inspired log fields so they can be
shipped directly to collectors such as the OpenTelemetry Collector, Loki, or
CloudWatch without regex parsing.

## Architecture

- `src/observability/logger.js` owns log formatting and output.
- `requestLogger` is registered before application routes, assigns or preserves
  `x-request-id`, and emits a completion record when the response finishes.
- HTTP attributes follow stable OpenTelemetry semantic convention names such as
  `http.request.method`, `http.response.status_code`, `http.route`, `url.path`,
  `server.address`, `client.address`, and `user_agent.original`.
- Resource attributes include `service.name`, `service.version`, and
  `deployment.environment.name`.

## Operations

### Monitoring and dashboards

Recommended panels:

1. Request rate by `http.route` and `http.response.status_code`.
2. P95/P99 latency from `event.duration_ms`, with critical-path alerts when P99
   stays above 100 ms for five minutes.
3. Error ratio by route, alerting when 5xx responses exceed 1% over five
   minutes.
4. Legal-hold blocked actions by route and 502 status.

### Alerting

Page on sustained 5xx error ratio or latency SLO breach. Create a warning alert
for repeated 4xx spikes because they can indicate client integration drift or
probing traffic.

### Deployment

Use blue-green deployment with canary analysis before full cutover:

1. Send 5% of traffic to the green environment.
2. Compare request count, 5xx rate, and P99 latency from structured logs.
3. Increase traffic to 25%, 50%, then 100% only when SLOs remain healthy.
4. Roll back to blue if 5xx ratio or P99 latency breaches the thresholds above.

### Security

Log records intentionally avoid request bodies, authorization headers, cookies,
and stack traces. Keep any future attributes free of secrets or personal data
unless a documented security review approves them.
