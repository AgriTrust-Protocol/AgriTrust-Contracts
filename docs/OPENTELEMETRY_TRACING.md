# OpenTelemetry Distributed Tracing

AgriTrust API requests now carry W3C Trace Context from ingress to downstream
handlers. The Express middleware emits OpenTelemetry-compatible JSON span events
and returns the active `traceparent` header to callers so critical escrow paths
can be correlated across services without adding blocking network calls to the
request path.

## Architecture

1. **Ingress propagation**: `src/middleware/tracing.js` accepts `traceparent` and
   `tracestate` request headers. Valid incoming trace IDs are reused and a fresh
   server span ID is generated for each request.
2. **Application context**: the active context is attached to `req.traceContext`
   for route handlers and future on-chain adapters to pass into outbound calls.
3. **Egress propagation**: responses include a new `traceparent` header. If the
   caller supplied `tracestate`, it is preserved in the response.
4. **Span emission**: when the response finishes, the middleware logs a compact
   JSON event named `otel.http.server.span` with OpenTelemetry semantic
   attributes for method, route, status code, URL path, service name, trace ID,
   span ID, parent span ID, and duration.
5. **Collector pipeline**: production log collectors should route these JSON
   events to the OpenTelemetry Collector and export to the configured tracing
   backend. Keeping export out of the synchronous request path protects the
   <100ms P99 target and avoids tracing outages affecting API availability.

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `OTEL_SERVICE_NAME` | `agritrust-contracts-api` | Service name included on emitted spans. |
| `OTEL_TRACES_ENABLED` | `true` | Set to `false` to keep header propagation while suppressing span logs. |

## Monitoring and alerting

Create dashboards from `otel.http.server.span` events with these panels:

- Request rate by `service_name`, `attributes.http.route`, and
  `attributes.http.response.status_code`.
- P50/P95/P99 latency from `duration_ms`, with a critical-path P99 alert at
  100ms for `/escrow/:escrowId` and mutating escrow routes.
- Error ratio alert for 5xx responses above 1% over five minutes.
- Trace sampling coverage by checking the low bit of `trace_flags` in
  `traceparent`; production ingress should sample enough traffic for incident
  forensics while avoiding sensitive payload capture.

## Security and privacy

- Trace IDs and span IDs are random hex identifiers and do not encode escrow IDs,
  account addresses, balances, or request bodies.
- Logs intentionally include route templates and URL paths, but not request body
  content or stack traces.
- Invalid inbound trace context is ignored and replaced with a fresh trace so
  malformed or all-zero headers cannot poison correlation data.

## Deployment runbook

1. Deploy behind a blue-green load balancer with `OTEL_TRACES_ENABLED=false` in
   green to validate propagation headers without span ingestion volume.
2. Send synthetic GET and POST escrow requests with a known `traceparent` header
   and confirm the response preserves the trace ID.
3. Enable `OTEL_TRACES_ENABLED=true` for a 5% canary and verify collector ingest,
   dashboard panels, and the P99 latency budget.
4. Increase canary traffic to 25%, 50%, then 100% if 5xx rate, collector lag,
   and P99 latency remain within SLOs.
5. Roll back by setting `OTEL_TRACES_ENABLED=false`; propagation remains active,
   so downstream services continue receiving trace context during mitigation.
