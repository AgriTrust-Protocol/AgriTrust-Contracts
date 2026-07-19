"use strict";

const DEFAULT_SLO = Object.freeze({
  latencyTargetMs: 100,
  availabilityTarget: 0.9999,
  shortWindowMinutes: 5,
  longWindowMinutes: 60,
  pageBurnRate: 14.4,
  ticketBurnRate: 6,
});

function percentile(values, p) {
  if (!Array.isArray(values) || values.length === 0) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const index = Math.ceil((p / 100) * sorted.length) - 1;
  return sorted[Math.min(sorted.length - 1, Math.max(0, index))];
}

function errorBudget(availabilityTarget) {
  return Math.max(0, 1 - availabilityTarget);
}

function computeBurnRate({ totalRequests, failedRequests, availabilityTarget = DEFAULT_SLO.availabilityTarget }) {
  if (!totalRequests || totalRequests <= 0) return 0;
  const observedErrorRate = failedRequests / totalRequests;
  const budget = errorBudget(availabilityTarget);
  return budget === 0 ? 0 : observedErrorRate / budget;
}

function evaluateWindow({ samples, config = {} }) {
  const slo = { ...DEFAULT_SLO, ...config };
  const safeSamples = Array.isArray(samples) ? samples : [];
  const totalRequests = safeSamples.reduce((sum, sample) => sum + (sample.requests || 0), 0);
  const failedRequests = safeSamples.reduce((sum, sample) => sum + (sample.errors || 0), 0);
  const latencyValues = safeSamples.flatMap((sample) => sample.latenciesMs || []);
  const p99LatencyMs = percentile(latencyValues, 99);
  const availability = totalRequests === 0 ? 1 : 1 - failedRequests / totalRequests;
  const burnRate = computeBurnRate({ totalRequests, failedRequests, availabilityTarget: slo.availabilityTarget });

  return {
    totalRequests,
    failedRequests,
    availability,
    p99LatencyMs,
    latencyObjectiveMet: p99LatencyMs < slo.latencyTargetMs,
    availabilityObjectiveMet: availability >= slo.availabilityTarget,
    burnRate,
  };
}

function evaluateSlo({ shortWindowSamples, longWindowSamples, config = {} }) {
  const slo = { ...DEFAULT_SLO, ...config };
  const shortWindow = evaluateWindow({ samples: shortWindowSamples, config: slo });
  const longWindow = evaluateWindow({ samples: longWindowSamples, config: slo });
  const page = shortWindow.burnRate >= slo.pageBurnRate && longWindow.burnRate >= slo.pageBurnRate;
  const ticket = !page && shortWindow.burnRate >= slo.ticketBurnRate && longWindow.burnRate >= slo.ticketBurnRate;
  const latencyPage = shortWindow.p99LatencyMs >= slo.latencyTargetMs && longWindow.p99LatencyMs >= slo.latencyTargetMs;

  return {
    objective: {
      latencyP99Ms: slo.latencyTargetMs,
      availability: slo.availabilityTarget,
    },
    shortWindow,
    longWindow,
    alert: page || latencyPage ? "page" : ticket ? "ticket" : "none",
    reasons: [
      page ? "availability_error_budget_page_burn" : null,
      ticket ? "availability_error_budget_ticket_burn" : null,
      latencyPage ? "critical_path_p99_latency_breach" : null,
    ].filter(Boolean),
  };
}

function prometheusMetrics(result, labels = {}) {
  const labelText = Object.entries(labels)
    .map(([key, value]) => `${key}="${String(value).replace(/"/g, "\\\"")}"`)
    .join(",");
  const suffix = labelText ? `{${labelText}}` : "";
  const alertValue = result.alert === "page" ? 2 : result.alert === "ticket" ? 1 : 0;

  return [
    `agritrust_slo_availability_ratio${suffix} ${result.shortWindow.availability}`,
    `agritrust_slo_p99_latency_ms${suffix} ${result.shortWindow.p99LatencyMs}`,
    `agritrust_slo_burn_rate_short${suffix} ${result.shortWindow.burnRate}`,
    `agritrust_slo_burn_rate_long${suffix} ${result.longWindow.burnRate}`,
    `agritrust_slo_alert_state${suffix} ${alertValue}`,
  ].join("\n");
}

module.exports = {
  DEFAULT_SLO,
  computeBurnRate,
  evaluateSlo,
  evaluateWindow,
  percentile,
  prometheusMetrics,
};
