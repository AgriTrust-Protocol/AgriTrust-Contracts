"use strict";

const {
  ConsumerGroupAutoScaler,
  buildLagAlert,
  calculateConsumerGroupLag,
} = require("../src/services/kafkaLagMonitor");

describe("calculateConsumerGroupLag", () => {
  it("aggregates total, max partition, and per-topic lag", () => {
    const result = calculateConsumerGroupLag([
      { topic: "escrow-events", partition: 0, currentOffset: 90, highWatermark: 100 },
      { topic: "escrow-events", partition: 1, currentOffset: 50, highWatermark: 100 },
      { topic: "payout-events", partition: 0, currentOffset: 20, highWatermark: 25 },
    ]);

    expect(result.totalLag).toBe(65);
    expect(result.maxPartitionLag).toBe(50);
    expect(result.lagByTopic).toEqual({ "escrow-events": 60, "payout-events": 5 });
  });

  it("floors negative lag at zero for compacted or reset partitions", () => {
    const result = calculateConsumerGroupLag([
      { topic: "escrow-events", partition: 0, currentOffset: 120, highWatermark: 100 },
    ]);

    expect(result.totalLag).toBe(0);
  });
});

describe("ConsumerGroupAutoScaler", () => {
  it("recommends scale up based on target lag per replica", () => {
    const scaler = new ConsumerGroupAutoScaler({ now: () => 1_000_000 });
    const decision = scaler.recommend({ consumerGroup: "escrow-writer", currentReplicas: 2, lag: 12500 });

    expect(decision).toEqual({ action: "scale_up", replicas: 5, reason: "lag_above_threshold" });
  });

  it("recommends scale down by one replica when lag is below threshold", () => {
    const scaler = new ConsumerGroupAutoScaler({ now: () => 1_000_000, minReplicas: 2 });
    const decision = scaler.recommend({ consumerGroup: "escrow-writer", currentReplicas: 4, lag: 500 });

    expect(decision).toEqual({ action: "scale_down", replicas: 3, reason: "lag_below_threshold" });
  });

  it("holds recommendations during cooldown", () => {
    let now = 1_000_000;
    const scaler = new ConsumerGroupAutoScaler({ now: () => now, cooldownMs: 120000 });

    scaler.recommend({ consumerGroup: "escrow-writer", currentReplicas: 2, lag: 12500 });
    now += 60000;

    expect(scaler.recommend({ consumerGroup: "escrow-writer", currentReplicas: 5, lag: 30000 })).toEqual({
      action: "hold",
      replicas: 5,
      reason: "cooldown",
    });
  });

  it("never exceeds maxReplicas", () => {
    const scaler = new ConsumerGroupAutoScaler({ now: () => 1_000_000, maxReplicas: 6 });
    const decision = scaler.recommend({ consumerGroup: "escrow-writer", currentReplicas: 5, lag: 100000 });

    expect(decision.replicas).toBe(6);
  });
});

describe("buildLagAlert", () => {
  it("returns a warning alert when lag breaches threshold", () => {
    expect(buildLagAlert({ consumerGroup: "escrow-writer", lag: 15000, threshold: 10000 })).toMatchObject({
      severity: "warning",
      name: "KafkaConsumerLagHigh",
      consumerGroup: "escrow-writer",
    });
  });

  it("does not alert below threshold", () => {
    expect(buildLagAlert({ consumerGroup: "escrow-writer", lag: 9999, threshold: 10000 })).toBeNull();
  });
});
