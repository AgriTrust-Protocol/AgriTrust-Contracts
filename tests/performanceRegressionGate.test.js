"use strict";

const {
  DEFAULT_POLICY,
  P99_CEILING_MS,
  REGRESSION_THRESHOLD,
  THROUGHPUT_DROP_THRESHOLD,
  checkAbsoluteCeiling,
  compareToBaseline,
  evaluate,
  parseReport,
  prometheusMetrics,
} = require("../scripts/performance-regression-gate");

// ── parseReport ───────────────────────────────────────────────────────────────

describe("parseReport", () => {
  it("accepts a well-formed report and normalises optional fields", () => {
    const benchmarks = parseReport({
      benchmarks: [
        { name: "grant_stream_invoke", p99Ms: 45, p50Ms: 20, throughputRps: 500 },
        { name: "vesting_claim", p99Ms: 80 },
      ],
    });

    expect(benchmarks).toHaveLength(2);
    expect(benchmarks[0]).toMatchObject({ name: "grant_stream_invoke", p99Ms: 45, p50Ms: 20, throughputRps: 500 });
    expect(benchmarks[1]).toMatchObject({ name: "vesting_claim", p99Ms: 80, p50Ms: null, throughputRps: null });
  });

  it("trims whitespace from benchmark names", () => {
    const benchmarks = parseReport({ benchmarks: [{ name: "  escrow_release  ", p99Ms: 30 }] });
    expect(benchmarks[0].name).toBe("escrow_release");
  });

  it("throws when the root value is not an object", () => {
    expect(() => parseReport(null)).toThrow(TypeError);
    expect(() => parseReport("string")).toThrow(TypeError);
  });

  it("throws when benchmarks is not an array", () => {
    expect(() => parseReport({ benchmarks: "not-an-array" })).toThrow(TypeError);
  });

  it("throws when a benchmark entry is not an object", () => {
    expect(() => parseReport({ benchmarks: [42] })).toThrow(TypeError);
  });

  it("throws when a benchmark name is missing or empty", () => {
    expect(() => parseReport({ benchmarks: [{ p99Ms: 50 }] })).toThrow(TypeError);
    expect(() => parseReport({ benchmarks: [{ name: "  ", p99Ms: 50 }] })).toThrow(TypeError);
  });

  it("throws when p99Ms is missing, negative, or non-finite", () => {
    expect(() => parseReport({ benchmarks: [{ name: "x" }] })).toThrow(TypeError);
    expect(() => parseReport({ benchmarks: [{ name: "x", p99Ms: -1 }] })).toThrow(TypeError);
    expect(() => parseReport({ benchmarks: [{ name: "x", p99Ms: Infinity }] })).toThrow(TypeError);
  });
});

// ── checkAbsoluteCeiling ──────────────────────────────────────────────────────

describe("checkAbsoluteCeiling", () => {
  it("passes when P99 is strictly below the ceiling", () => {
    expect(checkAbsoluteCeiling({ name: "a", p99Ms: 99 }, { p99CeilingMs: 100 }).passed).toBe(true);
  });

  it("fails when P99 equals the ceiling", () => {
    const result = checkAbsoluteCeiling({ name: "a", p99Ms: 100 }, { p99CeilingMs: 100 });
    expect(result.passed).toBe(false);
    expect(result.reason).toMatch(/100 ms/);
  });

  it("fails when P99 exceeds the ceiling", () => {
    const result = checkAbsoluteCeiling({ name: "a", p99Ms: 150 }, { p99CeilingMs: 100 });
    expect(result.passed).toBe(false);
    expect(result.reason).toMatch(/150 ms/);
  });

  it("uses the policy ceiling from DEFAULT_POLICY constants", () => {
    expect(P99_CEILING_MS).toBe(100);
  });
});

// ── compareToBaseline ─────────────────────────────────────────────────────────

describe("compareToBaseline", () => {
  const policy = { regressionThreshold: 0.10, throughputDropThreshold: 0.20 };

  it("returns no regressions when current is within the threshold", () => {
    const { regressions } = compareToBaseline(
      { name: "a", p99Ms: 88, throughputRps: 480 },
      { name: "a", p99Ms: 80, throughputRps: 500 },
      policy
    );
    expect(regressions).toHaveLength(0);
  });

  it("flags a P99 regression when current exceeds baseline × (1 + threshold)", () => {
    const { regressions } = compareToBaseline(
      { name: "a", p99Ms: 89, throughputRps: null },
      { name: "a", p99Ms: 80, throughputRps: null },
      policy
    );
    expect(regressions).toHaveLength(1);
    expect(regressions[0]).toMatch(/regressed/i);
    expect(regressions[0]).toMatch(/89 ms/);
  });

  it("warns on throughput degradation above the drop threshold", () => {
    const { warnings, regressions } = compareToBaseline(
      { name: "a", p99Ms: 80, throughputRps: 300 },
      { name: "a", p99Ms: 80, throughputRps: 500 },
      policy
    );
    expect(regressions).toHaveLength(0);
    expect(warnings).toHaveLength(1);
    expect(warnings[0]).toMatch(/throughput/i);
    expect(warnings[0]).toMatch(/40\.0%/);
  });

  it("does not warn when throughput drop is at or below the threshold", () => {
    const { warnings } = compareToBaseline(
      { name: "a", p99Ms: 50, throughputRps: 400 },
      { name: "a", p99Ms: 50, throughputRps: 500 },
      policy
    );
    expect(warnings).toHaveLength(0);
  });

  it("skips throughput comparison when either side is null", () => {
    const { warnings } = compareToBaseline(
      { name: "a", p99Ms: 50, throughputRps: null },
      { name: "a", p99Ms: 50, throughputRps: 500 },
      policy
    );
    expect(warnings).toHaveLength(0);
  });
});

// ── evaluate ──────────────────────────────────────────────────────────────────

describe("evaluate", () => {
  it("passes when all benchmarks are under the ceiling with no baseline", () => {
    const current = parseReport({
      benchmarks: [
        { name: "grant_stream_invoke", p99Ms: 45, throughputRps: 600 },
        { name: "vesting_claim", p99Ms: 75 },
      ],
    });
    const result = evaluate(current, null);

    expect(result.passed).toBe(true);
    expect(result.summary.failed).toBe(0);
    expect(result.summary.baselineCompared).toBe(false);
  });

  it("fails when a benchmark breaches the absolute P99 ceiling", () => {
    const current = parseReport({
      benchmarks: [{ name: "slow_path", p99Ms: 120 }],
    });
    const result = evaluate(current, null);

    expect(result.passed).toBe(false);
    expect(result.summary.failed).toBe(1);
    expect(result.results[0].failures).toHaveLength(1);
    expect(result.results[0].failures[0]).toMatch(/120 ms/);
  });

  it("fails when current regresses beyond the threshold from baseline", () => {
    const current = parseReport({ benchmarks: [{ name: "escrow_release", p99Ms: 89 }] });
    const baseline = parseReport({ benchmarks: [{ name: "escrow_release", p99Ms: 80 }] });

    const result = evaluate(current, baseline);

    expect(result.passed).toBe(false);
    expect(result.results[0].failures[0]).toMatch(/regressed/i);
  });

  it("passes when P99 improves from baseline", () => {
    const current = parseReport({ benchmarks: [{ name: "arbitration_resolve", p99Ms: 60 }] });
    const baseline = parseReport({ benchmarks: [{ name: "arbitration_resolve", p99Ms: 80 }] });

    const result = evaluate(current, baseline);
    expect(result.passed).toBe(true);
    expect(result.summary.warnings).toBe(0);
  });

  it("handles a new benchmark not present in baseline without failing", () => {
    const current = parseReport({
      benchmarks: [
        { name: "existing", p99Ms: 50 },
        { name: "new_benchmark", p99Ms: 40 },
      ],
    });
    const baseline = parseReport({ benchmarks: [{ name: "existing", p99Ms: 50 }] });

    const result = evaluate(current, baseline);
    expect(result.passed).toBe(true);
    expect(result.results.find((r) => r.name === "new_benchmark").baselineP99Ms).toBeNull();
  });

  it("reports warnings for throughput degradation without failing the gate", () => {
    const current = parseReport({ benchmarks: [{ name: "api", p99Ms: 50, throughputRps: 300 }] });
    const baseline = parseReport({ benchmarks: [{ name: "api", p99Ms: 50, throughputRps: 500 }] });

    const result = evaluate(current, baseline);
    expect(result.passed).toBe(true);
    expect(result.summary.warnings).toBe(1);
  });

  it("accumulates both ceiling and regression failures for the same benchmark", () => {
    const current = parseReport({ benchmarks: [{ name: "dual_fail", p99Ms: 115 }] });
    const baseline = parseReport({ benchmarks: [{ name: "dual_fail", p99Ms: 80 }] });

    const result = evaluate(current, baseline);
    expect(result.passed).toBe(false);
    // ceiling breach + regression breach = 2 failures
    expect(result.results[0].failures.length).toBeGreaterThanOrEqual(2);
  });

  it("uses custom policy overrides when provided", () => {
    const current = parseReport({ benchmarks: [{ name: "relaxed", p99Ms: 90 }] });
    const baseline = parseReport({ benchmarks: [{ name: "relaxed", p99Ms: 80 }] });

    // 15% regression threshold — 90ms is within 80ms × 1.15 = 92ms
    const result = evaluate(current, baseline, { regressionThreshold: 0.15, p99CeilingMs: 200 });
    expect(result.passed).toBe(true);
  });

  it("exposes summary counts for passed and failed benchmarks", () => {
    const current = parseReport({
      benchmarks: [
        { name: "fast", p99Ms: 30 },
        { name: "slow", p99Ms: 105 },
      ],
    });
    const result = evaluate(current, null);
    expect(result.summary.total).toBe(2);
    expect(result.summary.passed).toBe(1);
    expect(result.summary.failed).toBe(1);
  });
});

// ── prometheusMetrics ─────────────────────────────────────────────────────────

describe("prometheusMetrics", () => {
  it("emits a gate-passed gauge of 1 when all benchmarks pass", () => {
    const current = parseReport({ benchmarks: [{ name: "grant_stream_invoke", p99Ms: 50, throughputRps: 400 }] });
    const result = evaluate(current, null);

    const output = prometheusMetrics(result);
    expect(output).toContain("agritrust_perf_regression_gate_passed 1");
  });

  it("emits a gate-passed gauge of 0 when any benchmark fails", () => {
    const current = parseReport({ benchmarks: [{ name: "slow_path", p99Ms: 200 }] });
    const result = evaluate(current, null);

    const output = prometheusMetrics(result);
    expect(output).toContain("agritrust_perf_regression_gate_passed 0");
  });

  it("emits per-benchmark P99 latency gauges with label", () => {
    const current = parseReport({ benchmarks: [{ name: "vesting_claim", p99Ms: 65 }] });
    const result = evaluate(current, null);

    const output = prometheusMetrics(result);
    expect(output).toContain('agritrust_perf_benchmark_p99_ms{benchmark="vesting_claim"} 65');
  });

  it("emits baseline P99 when a baseline entry is present", () => {
    const current = parseReport({ benchmarks: [{ name: "escrow_release", p99Ms: 70 }] });
    const baseline = parseReport({ benchmarks: [{ name: "escrow_release", p99Ms: 60 }] });
    const result = evaluate(current, baseline);

    const output = prometheusMetrics(result);
    expect(output).toContain('agritrust_perf_benchmark_baseline_p99_ms{benchmark="escrow_release"} 60');
  });

  it("emits throughput gauge when throughputRps is provided", () => {
    const current = parseReport({ benchmarks: [{ name: "api", p99Ms: 40, throughputRps: 750 }] });
    const result = evaluate(current, null);

    const output = prometheusMetrics(result);
    expect(output).toContain('agritrust_perf_benchmark_throughput_rps{benchmark="api"} 750');
  });

  it("escapes double quotes in benchmark names", () => {
    const benchmarks = [{ name: 'say "hello"', p99Ms: 30, p50Ms: null, throughputRps: null }];
    const result = evaluate(benchmarks, null);

    const output = prometheusMetrics(result);
    expect(output).toContain('benchmark="say \\"hello\\""');
  });
});

// ── Policy constants ──────────────────────────────────────────────────────────

describe("policy constants", () => {
  it("enforces <100 ms P99 ceiling matching the issue specification", () => {
    expect(DEFAULT_POLICY.p99CeilingMs).toBe(100);
    expect(P99_CEILING_MS).toBe(100);
  });

  it("uses a 10% regression threshold by default", () => {
    expect(DEFAULT_POLICY.regressionThreshold).toBe(REGRESSION_THRESHOLD);
    expect(REGRESSION_THRESHOLD).toBe(0.10);
  });

  it("uses a 20% throughput drop threshold by default", () => {
    expect(DEFAULT_POLICY.throughputDropThreshold).toBe(THROUGHPUT_DROP_THRESHOLD);
    expect(THROUGHPUT_DROP_THRESHOLD).toBe(0.20);
  });
});
