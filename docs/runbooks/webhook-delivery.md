# Webhook Delivery Runbook

## Alerts

Trigger an incident when any of the following hold for five minutes:

- Delivery failure rate is above 1%.
- Dead letter queue pending entries increase for two consecutive evaluation windows.
- Retry attempt rate doubles over the trailing one-hour baseline.
- Signature verification failures spike above normal partner onboarding levels.
- P99 local delivery latency exceeds 100ms before network time.

## Triage

1. Check `GET /webhooks/metrics` for `failed`, `attempts`, `delivered`, `deadLettered`, `deadLetterQueue.pending`, and `signatureVerificationFailed` deltas.
2. Inspect `GET /webhooks/dead-letter` for affected message ids, queues, reasons, attempt counts, and redacted payload metadata.
3. Confirm endpoint DNS/TLS health with the affected partner.
4. Verify the partner is using the current signing secret and the five-minute timestamp tolerance.
5. Pause non-critical webhook subscriptions if retries threaten API saturation.

## Recovery

1. Keep the blue deployment serving if canary analysis shows regressions in green.
2. Replay failed events from the DLQ once the partner endpoint is healthy and canary replay succeeds.
3. Rotate endpoint secrets if signatures fail after a suspected secret leak.
4. Document the incident, blast radius, and replay totals in the post-incident review.
