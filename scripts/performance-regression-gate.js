#!/usr/bin/env node
"use strict";

/**
 * Performance Regression Gate
 *
 * Compares benchmark results against a stored baseline and exits non-zero
 * when any critical-path metric regresses beyond the allowed threshold.
 *
 * Usage:
 *   node scripts/performance-regression-gate.js <current.json> [baseline.json]
 *
 * Report schema (both current and baseline files must conform):
 * {
 *   "benchmarks": [
 *     {
 *       "name": "string",          // unique benchmark identifier
 *       "p99Ms": number,           // P99 latency in milliseconds
 *       "p50Ms": number,           // P50 (median) latency in milliseconds
 *       "throughputRps": number    // requests per second
 *     }
 *   ]
 * }
 */

const fs = require("fs");

// ── Policy constants ──────────────────────────────────────────────────────────

/** Absolute P99 ceiling for every critical-path benchmark (ms). Issue target: <100 ms */
const P99_CEILING_MS = 100;

/**
 * Maximum allowed regression from baseline before the gate fails (fractional).
 * 0.10 = 10 % regression; e.g. baseline p99 = 80 ms → threshold = 88 ms.
 */
const REGRESSION_THRESHOLD = 0.10;

/**
 * Minimum throughput degradation that triggers a warning.
 * 0.20 = a 20 % drop in throughput is flagged.
 */
const THROUGHPUT_DROP_THRESHOLD = 0.20;

const DEFAULT_POLICY = Object.freeze({
  p99CeilingMs: P99_CEILING_MS,
  regressionThreshold: REGRESSION_THRESHOLD,
  throughputDropThreshold: THROUGHPUT_DROP_THRESHOLD,
});

// ── Parsing ───────────────────────────────────────────────────────────────────

/**
 * Validates and returns the benchmarks array from a parsed report object.
 * Throws if the structure is missing or malformed.
 *
 * @param {unknown} report
 * @returns {{ name: string; p99Ms: number; p50Ms?: number; throughputRps?: number }[]}
 */
function parseReport(report) {
  if (!report || typeof report !== "object") {
    throw new TypeError("Report must be a JSON object.");
  }
  if (!Array.isArray(report.benchmarks)) {
    throw new TypeError('Report must contain a "benchmarks" array.');
  }
  return report.benchmarks.map((entry, index) => {
    if (!entry || typeof entry !== "object") {
      throw new TypeError(`benchmarks[${index}] is not an object.`);
    }
    if (typeof entry.name !== "string" || entry.name.trim() === "") {
      throw new TypeError(`benchmarks[${index}].name must be a non-empty string.`);
    }
    if (typeof entry.p99Ms !== "number" || !Number.isFinite(entry.p99Ms) || entry.p99Ms < 0) {
      throw new TypeError(`benchmarks[${index}].p99Ms must be a non-negative finite number.`);
    }
    return {
      name: entry.name.trim(),
      p99Ms: entry.p99Ms,
      p50Ms: typeof entry.p50Ms === "number" && Number.isFinite(entry.p50Ms) ? entry.p50Ms : null,
      throughputRps:
        typeof entry.throughputRps === "number" && Number.isFinite(entry.throughputRps)
          ? entry.throughputRps
          : null,
    };
  });
}

// ── Regression analysis ───────────────────────────────────────────────────────

/**
 * Checks a single benchmark against the absolute P99 ceiling.
 *
 * @param {{ name: string; p99Ms: number }} benchmark
 * @param {{ p99CeilingMs: number }} policy
 * @returns {{ passed: boolean; reason?: string }}
 */
function checkAbsoluteCeiling(benchmark, policy) {
  if (benchmark.p99Ms >= policy.p99CeilingMs) {
    return {
      passed: false,
      reason: `P99 latency ${benchmark.p99Ms} ms exceeds the ${policy.p99CeilingMs} ms critical-path ceiling`,
    };
  }
  return { passed: true };
}

/**
 * Compares a benchmark against its baseline entry and flags regressions.
 *
 * @param {{ name: string; p99Ms: number; throughputRps: number|null }} current
 * @param {{ name: string; p99Ms: number; throughputRps: number|null }} baseline
 * @param {{ regressionThreshold: number; throughputDropThreshold: number }} policy
 * @returns {{ regressions: string[]; warnings: string[] }}
 */
function compareToBaseline(current, baseline, policy) {
  const regressions = [];
  const warnings = [];

  // P99 latency regression
  const p99Ceiling = baseline.p99Ms * (1 + policy.regressionThreshold);
  if (current.p99Ms > p99Ceiling) {
    regressions.push(
      `P99 latency regressed from ${baseline.p99Ms} ms to ${current.p99Ms} ms ` +
        `(allowed ceiling: ${p99Ceiling.toFixed(2)} ms, threshold: ${policy.regressionThreshold * 100}%)`
    );
  }

  // Throughput degradation warning (non-blocking by default, surfaced as warning)
  if (
    current.throughputRps !== null &&
    baseline.throughputRps !== null &&
    baseline.throughputRps > 0
  ) {
    const drop = (baseline.throughputRps - current.throughputRps) / baseline.throughputRps;
    if (drop > policy.throughputDropThreshold) {
      warnings.push(
        `Throughput dropped from ${baseline.throughputRps} rps to ${current.throughputRps} rps ` +
          `(${(drop * 100).toFixed(1)}% degradation, warning threshold: ${policy.throughputDropThreshold * 100}%)`
      );
    }
  }

  return { regressions, warnings };
}

/**
 * Evaluates all current benchmarks against the baseline and the absolute ceiling.
 *
 * @param {ReturnType<parseReport>} current
 * @param {ReturnType<parseReport> | null} baseline  Pass null to skip baseline comparison.
 * @param {typeof DEFAULT_POLICY} policy
 * @returns {{ passed: boolean; results: object[]; summary: object }}
 */
function evaluate(current, baseline, policy = DEFAULT_POLICY) {
  const pol = { ...DEFAULT_POLICY, ...policy };
  const baselineMap = new Map((baseline || []).map((b) => [b.name, b]));

  const results = [];
  let totalFailures = 0;
  let totalWarnings = 0;

  for (const benchmark of current) {
    const ceilingCheck = checkAbsoluteCeiling(benchmark, pol);
    const baselineEntry = baselineMap.get(benchmark.name) || null;
    const { regressions, warnings } =
      baselineEntry !== null
        ? compareToBaseline(benchmark, baselineEntry, pol)
        : { regressions: [], warnings: [] };

    const failures = [
      ...(ceilingCheck.passed ? [] : [ceilingCheck.reason]),
      ...regressions,
    ];

    results.push({
      name: benchmark.name,
      p99Ms: benchmark.p99Ms,
      p50Ms: benchmark.p50Ms,
      throughputRps: benchmark.throughputRps,
      baselineP99Ms: baselineEntry ? baselineEntry.p99Ms : null,
      passed: failures.length === 0,
      failures,
      warnings,
    });

    totalFailures += failures.length;
    totalWarnings += warnings.length;
  }

  return {
    passed: totalFailures === 0,
    policy: pol,
    results,
    summary: {
      total: current.length,
      passed: results.filter((r) => r.passed).length,
      failed: results.filter((r) => !r.passed).length,
      warnings: totalWarnings,
      baselineCompared: baseline !== null,
    },
  };
}

// ── Prometheus metrics export ─────────────────────────────────────────────────

/**
 * Serialises evaluation results as Prometheus-compatible text exposition.
 *
 * @param {ReturnType<evaluate>} result
 * @returns {string}
 */
function prometheusMetrics(result) {
  const lines = [];
  lines.push(
    `# HELP agritrust_perf_regression_gate_passed 1 if all benchmarks passed, 0 otherwise`,
    `# TYPE agritrust_perf_regression_gate_passed gauge`,
    `agritrust_perf_regression_gate_passed ${result.passed ? 1 : 0}`
  );

  lines.push(
    `# HELP agritrust_perf_benchmark_p99_ms P99 latency for a benchmark run`,
    `# TYPE agritrust_perf_benchmark_p99_ms gauge`
  );
  for (const r of result.results) {
    const label = `benchmark="${r.name.replace(/"/g, '\\"')}"`;
    lines.push(`agritrust_perf_benchmark_p99_ms{${label}} ${r.p99Ms}`);
    if (r.baselineP99Ms !== null) {
      lines.push(`agritrust_perf_benchmark_baseline_p99_ms{${label}} ${r.baselineP99Ms}`);
    }
    if (r.throughputRps !== null) {
      lines.push(`agritrust_perf_benchmark_throughput_rps{${label}} ${r.throughputRps}`);
    }
    lines.push(`agritrust_perf_benchmark_passed{${label}} ${r.passed ? 1 : 0}`);
  }

  return lines.join("\n");
}

// ── Baseline persistence helpers ──────────────────────────────────────────────

/**
 * Writes current benchmark results as a new baseline file.
 *
 * @param {ReturnType<parseReport>} benchmarks
 * @param {string} outputPath
 */
function writeBaseline(benchmarks, outputPath) {
  const content = JSON.stringify({ benchmarks }, null, 2);
  fs.writeFileSync(outputPath, content, "utf8");
}

// ── CLI entry point ───────────────────────────────────────────────────────────

function main(argv) {
  const [currentPath, baselinePath] = argv;

  if (!currentPath) {
    console.error(
      "Usage: performance-regression-gate.js <current.json> [baseline.json]"
    );
    return 2;
  }

  let currentReport;
  try {
    currentReport = JSON.parse(fs.readFileSync(currentPath, "utf8"));
  } catch (err) {
    console.error(`Failed to read current report at "${currentPath}": ${err.message}`);
    return 2;
  }

  let current;
  try {
    current = parseReport(currentReport);
  } catch (err) {
    console.error(`Invalid current report: ${err.message}`);
    return 2;
  }

  let baseline = null;
  if (baselinePath) {
    try {
      const baselineReport = JSON.parse(fs.readFileSync(baselinePath, "utf8"));
      baseline = parseReport(baselineReport);
    } catch (err) {
      console.error(`Failed to read baseline at "${baselinePath}": ${err.message}`);
      return 2;
    }
  }

  const result = evaluate(current, baseline);

  console.log(JSON.stringify(result, null, 2));

  if (!result.passed) {
    console.error("\nPerformance gate FAILED. Regressions detected:");
    for (const r of result.results.filter((r) => !r.passed)) {
      for (const failure of r.failures) {
        console.error(`  [${r.name}] ${failure}`);
      }
    }
    return 1;
  }

  if (result.summary.warnings > 0) {
    console.warn("\nPerformance gate passed with warnings:");
    for (const r of result.results.filter((r) => r.warnings.length > 0)) {
      for (const warning of r.warnings) {
        console.warn(`  [${r.name}] ${warning}`);
      }
    }
  }

  console.log("\nPerformance gate PASSED.");
  return 0;
}

if (require.main === module) {
  process.exitCode = main(process.argv.slice(2));
}

module.exports = {
  DEFAULT_POLICY,
  P99_CEILING_MS,
  REGRESSION_THRESHOLD,
  THROUGHPUT_DROP_THRESHOLD,
  checkAbsoluteCeiling,
  compareToBaseline,
  evaluate,
  parseReport,
  prometheusMetrics,
  writeBaseline,
};
