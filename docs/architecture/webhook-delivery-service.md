# Webhook Delivery Service

## Architecture

The webhook delivery service signs every outbound event with an HMAC-SHA256 digest over `timestamp.payload`, posts JSON to the subscriber endpoint, and retries transient failures with bounded exponential backoff. The API also exposes an inbound verification helper for partners and internal services that need to validate AgriTrust webhook signatures.

## Critical path and availability

- Outbound signing is synchronous and uses deterministic JSON serialization so critical-path work stays below the 100ms P99 target before the network call is dispatched.
- Delivery attempts are isolated behind retry settings (`maxAttempts`, timeout, initial delay, factor, max delay) so callers can enqueue work and avoid blocking user-facing paths.
- Failed deliveries are counted separately from attempts to support 99.99% availability alerting and replay workflows.

## Security model

- Endpoint URLs must be HTTPS outside test environments.
- Endpoint secrets must be at least 16 characters.
- Signatures use `x-agritrust-webhook-timestamp` and `x-agritrust-webhook-signature: sha256=<hex>` headers.
- Verification rejects signatures outside a five-minute clock-skew window to reduce replay risk.
- Comparisons use timing-safe equality for same-length hex digests.

## Monitoring and dashboards

`GET /webhooks/metrics` returns counters for enqueued events, attempts, delivered events, failed events, signature verification failures, and P99 delivery latency. Dashboards should graph delivery success rate, retry rate, failure rate, signature verification failures, and P99 delivery latency against the 100ms target for local processing latency.

## Deployment

Deploy with blue-green application instances. Send a canary cohort of low-risk subscriptions to the green pool, compare delivery success rate and P99 latency against blue, and only then shift production traffic. Roll back if failures increase or signature verification failures spike unexpectedly.
