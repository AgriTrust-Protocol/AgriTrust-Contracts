"use strict";

const DEFAULTS = Object.freeze({
  scaleUpLagThreshold: 10000,
  scaleDownLagThreshold: 1000,
  maxReplicas: 30,
  minReplicas: 1,
  targetLagPerReplica: 2500,
  cooldownMs: 120000,
  now: () => Date.now(),
});

function assertNonNegativeInteger(value, name) {
  if (!Number.isInteger(value) || value < 0) {
    throw new TypeError(`${name} must be a non-negative integer`);
  }
}

function normalisePartitionLag(partition) {
  if (!partition || typeof partition !== "object") {
    throw new TypeError("partition lag sample must be an object");
  }

  const currentOffset = Number(partition.currentOffset);
  const highWatermark = Number(partition.highWatermark);
  if (!Number.isSafeInteger(currentOffset) || !Number.isSafeInteger(highWatermark)) {
    throw new TypeError("partition offsets must be safe integers");
  }

  return {
    topic: String(partition.topic || "unknown"),
    partition: Number(partition.partition || 0),
    currentOffset,
    highWatermark,
    lag: Math.max(0, highWatermark - currentOffset),
  };
}

function calculateConsumerGroupLag(samples) {
  if (!Array.isArray(samples)) {
    throw new TypeError("lag samples must be an array");
  }

  const partitions = samples.map(normalisePartitionLag);
  const lagByTopic = partitions.reduce((acc, sample) => {
    acc[sample.topic] = (acc[sample.topic] || 0) + sample.lag;
    return acc;
  }, {});

  return {
    totalLag: partitions.reduce((sum, sample) => sum + sample.lag, 0),
    maxPartitionLag: partitions.reduce((max, sample) => Math.max(max, sample.lag), 0),
    lagByTopic,
    partitions,
  };
}

class ConsumerGroupAutoScaler {
  constructor(options = {}) {
    this.options = { ...DEFAULTS, ...options };
    assertNonNegativeInteger(this.options.scaleUpLagThreshold, "scaleUpLagThreshold");
    assertNonNegativeInteger(this.options.scaleDownLagThreshold, "scaleDownLagThreshold");
    assertNonNegativeInteger(this.options.targetLagPerReplica, "targetLagPerReplica");
    this.lastDecisionAtByGroup = new Map();
  }

  recommend({ consumerGroup, currentReplicas, lag }) {
    if (!consumerGroup) throw new TypeError("consumerGroup is required");
    if (!Number.isInteger(currentReplicas) || currentReplicas < 1) {
      throw new TypeError("currentReplicas must be a positive integer");
    }
    assertNonNegativeInteger(lag, "lag");

    const now = this.options.now();
    const lastDecisionAt = this.lastDecisionAtByGroup.get(consumerGroup) || 0;
    const inCooldown = now - lastDecisionAt < this.options.cooldownMs;
    if (inCooldown) {
      return { action: "hold", replicas: currentReplicas, reason: "cooldown" };
    }

    if (lag >= this.options.scaleUpLagThreshold) {
      const desired = Math.ceil(lag / Math.max(1, this.options.targetLagPerReplica));
      const replicas = Math.min(this.options.maxReplicas, Math.max(currentReplicas + 1, desired));
      this.lastDecisionAtByGroup.set(consumerGroup, now);
      return { action: replicas > currentReplicas ? "scale_up" : "hold", replicas, reason: "lag_above_threshold" };
    }

    if (lag <= this.options.scaleDownLagThreshold && currentReplicas > this.options.minReplicas) {
      const replicas = Math.max(this.options.minReplicas, currentReplicas - 1);
      this.lastDecisionAtByGroup.set(consumerGroup, now);
      return { action: "scale_down", replicas, reason: "lag_below_threshold" };
    }

    return { action: "hold", replicas: currentReplicas, reason: "lag_within_band" };
  }
}

function buildLagAlert({ consumerGroup, lag, threshold, windowMinutes = 5 }) {
  if (lag < threshold) return null;
  return {
    severity: "warning",
    name: "KafkaConsumerLagHigh",
    consumerGroup,
    summary: `Consumer group ${consumerGroup} lag is ${lag}`,
    runbook: "docs/KAFKA_CONSUMER_LAG_AUTOSCALING.md#runbook",
    for: `${windowMinutes}m`,
  };
}

module.exports = {
  ConsumerGroupAutoScaler,
  buildLagAlert,
  calculateConsumerGroupLag,
};
