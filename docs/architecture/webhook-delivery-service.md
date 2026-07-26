# Webhook Delivery Service

## Architecture

The webhook delivery service signs every outbound event with an HMAC-SHA256 digest over `timestamp.payload`, posts JSON to the subscriber endpoint, and retries transient failures with bounded exponential backoff. The API also exposes an inbound verification helper for partners and internal services that need to validate AgriTrust webhook signatures.

## Critical path and availability

- Outbound signing is synchronous and uses deterministic JSON serialization so critical-path work stays below the 100ms P99 target before the network call is dispatched.
- Delivery attempts are isolated behind retry settings (`maxAttempts`, timeout, initial delay, factor, max delay) so callers can enqueue work and avoid blocking user-facing paths.
- Failed deliveries are counted separately from attempts and are written to a dead letter queue (DLQ) after retry exhaustion to support 99.99% availability alerting and replay workflows.

## Security model

- Endpoint URLs must be HTTPS outside test environments.
- Endpoint secrets must be at least 16 characters.
- Signatures use `x-agritrust-webhook-timestamp` and `x-agritrust-webhook-signature: sha256=<hex>` headers.
- Verification rejects signatures outside a five-minute clock-skew window to reduce replay risk.
- Comparisons use timing-safe equality for same-length hex digests.

## Dead letter queue

After `maxAttempts` is exhausted, webhook delivery stores a redacted DLQ entry containing the service, logical queue, message id, payload, reason, attempt count, endpoint metadata, creation time, and retention expiry. DLQ ids are deterministic SHA-256 fingerprints so repeat failures for the same message can be correlated without exposing endpoint secrets. Operators can inspect pending webhook DLQ records with `GET /webhooks/dead-letter` and replay them through the service-level handler once the downstream endpoint is healthy.

## Monitoring and dashboards

`GET /webhooks/metrics` returns counters for enqueued events, attempts, delivered events, failed events, DLQ writes, signature verification failures, P99 delivery latency, and DLQ pending/replayed/purged totals. Dashboards should graph delivery success rate, retry rate, failure rate, DLQ growth, signature verification failures, and P99 delivery latency against the 100ms target for local processing latency.

## Deployment

Deploy with blue-green application instances. Send a canary cohort of low-risk subscriptions to the green pool, compare delivery success rate and P99 latency against blue, and only then shift production traffic. Roll back if failures increase or signature verification failures spike unexpectedly.
