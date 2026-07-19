"use strict";

const {
  computeBurnRate,
  evaluateSlo,
  evaluateWindow,
  percentile,
  prometheusMetrics,
} = require("../src/services/sloMonitor");

describe("sloMonitor", () => {
  it("calculates percentile latency without mutating input", () => {
    const latencies = [10, 80, 20, 99, 101];
    expect(percentile(latencies, 99)).toBe(101);
    expect(latencies).toEqual([10, 80, 20, 99, 101]);
  });

  it("computes burn rate against the 99.99% availability error budget", () => {
    expect(computeBurnRate({ totalRequests: 100000, failedRequests: 10 })).toBeCloseTo(1);
    expect(computeBurnRate({ totalRequests: 0, failedRequests: 0 })).toBe(0);
  });

  it("evaluates availability and latency objectives for a sample window", () => {
    const result = evaluateWindow({
      samples: [
        { requests: 50000, errors: 1, latenciesMs: [20, 40, 80] },
        { requests: 50000, errors: 2, latenciesMs: [25, 45, 90] },
      ],
    });

    expect(result.totalRequests).toBe(100000);
    expect(result.failedRequests).toBe(3);
    expect(result.availabilityObjectiveMet).toBe(true);
    expect(result.latencyObjectiveMet).toBe(true);
    expect(result.p99LatencyMs).toBe(90);
  });

  it("raises a page when short and long windows exhaust budget quickly", () => {
    const poorSamples = [{ requests: 100000, errors: 200, latenciesMs: [40, 50, 60] }];
    const result = evaluateSlo({ shortWindowSamples: poorSamples, longWindowSamples: poorSamples });

    expect(result.alert).toBe("page");
    expect(result.reasons).toContain("availability_error_budget_page_burn");
  });

  it("raises a page when P99 latency breaches the critical path target", () => {
    const slowSamples = [{ requests: 100000, errors: 0, latenciesMs: [99, 100, 101, 120] }];
    const result = evaluateSlo({ shortWindowSamples: slowSamples, longWindowSamples: slowSamples });

    expect(result.alert).toBe("page");
    expect(result.reasons).toContain("critical_path_p99_latency_breach");
  });

  it("exports Prometheus-compatible SLO metrics with labels", () => {
    const result = evaluateSlo({
      shortWindowSamples: [{ requests: 1000, errors: 0, latenciesMs: [10] }],
      longWindowSamples: [{ requests: 1000, errors: 0, latenciesMs: [10] }],
    });

    expect(prometheusMetrics(result, { service: "api" })).toContain('agritrust_slo_alert_state{service="api"} 0');
  });
});
