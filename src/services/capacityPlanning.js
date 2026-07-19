"use strict";

const DEFAULTS = Object.freeze({
  windowMs: 60 * 60 * 1000,
  bucketMs: 60 * 1000,
  criticalP99TargetMs: 100,
  defaultCapacityPerMinute: 600,
  alertUtilizationThreshold: 0.8,
});

function percentile(values, percentileRank) {
  if (!values.length) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const index = Math.ceil((percentileRank / 100) * sorted.length) - 1;
  return sorted[Math.max(0, Math.min(sorted.length - 1, index))];
}

function round(value, digits = 2) {
  const factor = 10 ** digits;
  return Math.round(value * factor) / factor;
}

class CapacityPlanner {
  constructor(options = {}) {
    this.config = { ...DEFAULTS, ...options };
    this.now = options.now || Date.now;
    this.buckets = new Map();
  }

  bucketStart(timestampMs) {
    return Math.floor(timestampMs / this.config.bucketMs) * this.config.bucketMs;
  }

  getBucket(service, timestampMs) {
    const key = `${service}:${this.bucketStart(timestampMs)}`;
    if (!this.buckets.has(key)) {
      this.buckets.set(key, {
        service,
        bucket_start_ms: this.bucketStart(timestampMs),
        requests: 0,
        errors: 0,
        latency_samples_ms: [],
      });
    }
    return this.buckets.get(key);
  }

  recordUsage({ service = "api", statusCode = 200, latencyMs = 0, timestampMs = this.now() } = {}) {
    const safeService = String(service).replace(/[^A-Za-z0-9_.:-]/g, "_").slice(0, 128) || "api";
    const bucket = this.getBucket(safeService, timestampMs);
    bucket.requests += 1;
    if (Number(statusCode) >= 500) bucket.errors += 1;
    if (Number.isFinite(latencyMs) && latencyMs >= 0) bucket.latency_samples_ms.push(latencyMs);
    this.prune(timestampMs);
  }

  prune(nowMs = this.now()) {
    const cutoff = nowMs - this.config.windowMs;
    for (const [key, bucket] of this.buckets.entries()) {
      if (bucket.bucket_start_ms < cutoff) this.buckets.delete(key);
    }
  }

  serviceBuckets(service) {
    this.prune();
    return Array.from(this.buckets.values())
      .filter((bucket) => bucket.service === service)
      .sort((a, b) => a.bucket_start_ms - b.bucket_start_ms);
  }

  services() {
    this.prune();
    return [...new Set(Array.from(this.buckets.values()).map((bucket) => bucket.service))].sort();
  }

  trend(service, options = {}) {
    const capacityPerMinute = Number(options.capacityPerMinute || this.config.defaultCapacityPerMinute);
    const buckets = this.serviceBuckets(service);
    const totalRequests = buckets.reduce((sum, bucket) => sum + bucket.requests, 0);
    const totalErrors = buckets.reduce((sum, bucket) => sum + bucket.errors, 0);
    const requestCounts = buckets.map((bucket) => bucket.requests);
    const latencySamples = buckets.flatMap((bucket) => bucket.latency_samples_ms);
    const peakRpm = requestCounts.length ? Math.max(...requestCounts) : 0;
    const avgRpm = requestCounts.length ? totalRequests / requestCounts.length : 0;
    const p99LatencyMs = percentile(latencySamples, 99);
    const utilization = capacityPerMinute > 0 ? peakRpm / capacityPerMinute : 0;
    const projectedNextHourRequests = Math.ceil(avgRpm * 60);
    const recommendedCapacityPerMinute = Math.max(1, Math.ceil(peakRpm / this.config.alertUtilizationThreshold));

    return {
      service,
      window_minutes: Math.ceil(this.config.windowMs / 60000),
      bucket_minutes: Math.ceil(this.config.bucketMs / 60000),
      total_requests: totalRequests,
      error_rate: totalRequests ? round(totalErrors / totalRequests, 4) : 0,
      avg_rpm: round(avgRpm),
      peak_rpm: peakRpm,
      p99_latency_ms: p99LatencyMs,
      utilization: round(utilization, 4),
      projected_next_hour_requests: projectedNextHourRequests,
      recommended_capacity_per_minute: recommendedCapacityPerMinute,
      alerts: this.alerts({ utilization, p99LatencyMs }),
    };
  }

  alerts({ utilization, p99LatencyMs }) {
    const alerts = [];
    if (utilization >= this.config.alertUtilizationThreshold) {
      alerts.push({ severity: "warning", code: "capacity_utilization_high" });
    }
    if (p99LatencyMs > this.config.criticalP99TargetMs) {
      alerts.push({ severity: "critical", code: "p99_latency_target_breached" });
    }
    return alerts;
  }

  snapshot(options = {}) {
    const services = this.services();
    return {
      generated_at: new Date(this.now()).toISOString(),
      services: services.map((service) => this.trend(service, options)),
    };
  }

  prometheusMetrics(options = {}) {
    const lines = [];
    for (const trend of this.snapshot(options).services) {
      const labels = `{service="${trend.service}"}`;
      lines.push(`agritrust_capacity_peak_rpm${labels} ${trend.peak_rpm}`);
      lines.push(`agritrust_capacity_avg_rpm${labels} ${trend.avg_rpm}`);
      lines.push(`agritrust_capacity_utilization${labels} ${trend.utilization}`);
      lines.push(`agritrust_capacity_p99_latency_ms${labels} ${trend.p99_latency_ms}`);
      lines.push(`agritrust_capacity_recommended_per_minute${labels} ${trend.recommended_capacity_per_minute}`);
    }
    return lines.join("\n");
  }
}

function createCapacityRecorder(planner = new CapacityPlanner()) {
  return function capacityRecorder(req, res, next) {
    const started = Date.now();
    res.on("finish", () => {
      planner.recordUsage({
        service: req.baseUrl || req.path.split("/")[1] || "api",
        statusCode: res.statusCode,
        latencyMs: Date.now() - started,
      });
    });
    return next();
  };
}

module.exports = { CapacityPlanner, createCapacityRecorder, DEFAULTS, percentile };
