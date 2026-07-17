# Kafka Consumer Lag Monitoring and Auto-Scaling

## Architecture

AgriTrust services that consume Kafka topics publish partition-level offset samples to the monitoring plane. The lag monitor computes `highWatermark - currentOffset` per partition, aggregates lag per topic and consumer group, and feeds both alerts and auto-scaling decisions.

The implementation is intentionally dependency-light so every service can use the same logic in API workers, background processors, and deployment controllers without adding Kafka client coupling to critical request paths.

## Scaling policy

- Scale up when total consumer-group lag is at or above `10,000` messages.
- Scale down one replica at a time when lag is at or below `1,000` messages.
- Target `2,500` lagged messages per replica when calculating desired capacity.
- Respect per-group cooldown windows to prevent replica flapping.
- Enforce `minReplicas` and `maxReplicas` guardrails from deployment configuration.

## Monitoring and alerting

Dashboards should include:

- Total lag by consumer group.
- Maximum partition lag by topic.
- Replica count and scaling recommendations.
- Consume rate versus produce rate.
- P99 processing latency for critical consumers; target is below `100ms`.

Alerts should fire when lag remains above threshold for five minutes and link to this runbook.

## Deployment strategy

1. Deploy lag collection and dashboards to the green environment.
2. Mirror production Kafka offset metadata into green without committing offsets.
3. Run canary consumers for low-risk topics and compare lag, throughput, and error rates against blue.
4. Promote green only when P99 processing latency remains below `100ms`, lag converges, and error budgets are healthy.
5. Roll back by disabling auto-scaling decisions first, then shifting traffic back to blue.

## Runbook

1. Check the `KafkaConsumerLagHigh` alert for affected consumer group and topic.
2. Confirm broker health, partition skew, and downstream dependency latency.
3. If lag is increasing and dependencies are healthy, allow the auto-scaler to add replicas or manually raise `maxReplicas` for the affected group.
4. If lag is isolated to one partition, investigate poison messages and replay from the last known good offset.
5. After recovery, verify lag is inside the target band and reset any temporary scaling overrides.

## Security review notes

The monitor processes offsets and deployment metadata only. It must not log payload contents, secrets, customer PII, or private keys. Scaling controls should run with least-privilege permissions limited to the target consumer workloads.
