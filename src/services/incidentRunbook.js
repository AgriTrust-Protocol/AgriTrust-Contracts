"use strict";

const crypto = require("crypto");

const DEFAULT_SEVERITY = "warning";
const SEVERITIES = new Set(["info", "warning", "error", "critical"]);
const RUNBOOKS = Object.freeze({
  postgres_pool_exhaustion: {
    service: "postgres",
    summary: "Postgres pool exhaustion",
    runbook: "/docs/runbooks/postgres-pool-health.md",
    dashboard: "dashboards/postgres-pool-health",
    severity: "critical",
    steps: [
      "Enable read-only degradation for non-critical mutations.",
      "Inspect active connections and long-running transactions.",
      "Scale application workers only after database headroom is confirmed.",
    ],
  },
  webhook_delivery_degradation: {
    service: "webhooks",
    summary: "Webhook delivery degradation",
    runbook: "/docs/runbooks/webhook-delivery.md",
    dashboard: "dashboards/webhook-delivery",
    severity: "error",
    steps: [
      "Pause low-priority webhook dispatches.",
      "Check partner endpoint error rates and retry queues.",
      "Replay failed deliveries after partner recovery is confirmed.",
    ],
  },
  service_mesh_mtls_failure: {
    service: "service-mesh",
    summary: "Service mesh mTLS failure",
    runbook: "/docs/runbooks/service-mesh-mtls.md",
    dashboard: "dashboards/service-mesh-mtls",
    severity: "critical",
    steps: [
      "Shift traffic to the healthy color using blue-green routing.",
      "Validate certificate rotation status and workload identities.",
      "Run canary analysis before restoring full traffic.",
    ],
  },
});

const metrics = { triggered: 0, pagerDutyEvents: 0, failures: 0, totalLatencyMs: 0, latencies: [] };

function resetIncidentMetrics() {
  metrics.triggered = 0;
  metrics.pagerDutyEvents = 0;
  metrics.failures = 0;
  metrics.totalLatencyMs = 0;
  metrics.latencies = [];
}

function snapshotIncidentMetrics() {
  const sorted = [...metrics.latencies].sort((a, b) => a - b);
  const p99Index = sorted.length ? Math.min(sorted.length - 1, Math.ceil(sorted.length * 0.99) - 1) : 0;
  return {
    triggered: metrics.triggered,
    pagerDutyEvents: metrics.pagerDutyEvents,
    failures: metrics.failures,
    avgLatencyMs: metrics.triggered ? Number((metrics.totalLatencyMs / metrics.triggered).toFixed(3)) : 0,
    p99LatencyMs: sorted[p99Index] || 0,
  };
}

function listRunbooks() {
  return Object.entries(RUNBOOKS).map(([id, runbook]) => ({ id, ...runbook }));
}

function getRunbook(id) {
  return RUNBOOKS[id] ? { id, ...RUNBOOKS[id] } : null;
}

function buildPagerDutyEvent({ runbook, incidentKey, dedupKey, severity, details = {}, source = "agritrust-contracts-api" }) {
  return {
    routing_key: process.env.PAGERDUTY_ROUTING_KEY || "test-routing-key",
    event_action: "trigger",
    dedup_key: dedupKey || incidentKey,
    payload: {
      summary: runbook.summary,
      source,
      severity,
      component: runbook.service,
      custom_details: {
        runbook: runbook.runbook,
        dashboard: runbook.dashboard,
        steps: runbook.steps,
        ...details,
      },
    },
  };
}

async function triggerIncident(input, options = {}) {
  const started = process.hrtime.bigint();
  const runbook = getRunbook(input?.runbookId);
  if (!runbook) {
    const err = new Error("Unknown runbook");
    err.statusCode = 404;
    throw err;
  }
  const severity = SEVERITIES.has(input.severity) ? input.severity : (input.severity ? null : runbook.severity || DEFAULT_SEVERITY);
  if (!severity) {
    const err = new Error("Invalid severity");
    err.statusCode = 400;
    throw err;
  }

  const incidentKey = input.incidentKey || crypto.createHash("sha256").update(`${runbook.id}:${severity}:${JSON.stringify(input.details || {})}`).digest("hex").slice(0, 24);
  const event = buildPagerDutyEvent({ runbook, incidentKey, dedupKey: input.dedupKey, severity, details: input.details, source: input.source });
  const fetchImpl = options.fetchImpl || global.fetch;

  metrics.triggered += 1;
  try {
    if (options.dryRun || process.env.PAGERDUTY_DRY_RUN === "true" || !fetchImpl) {
      metrics.pagerDutyEvents += 1;
      return { ok: true, dryRun: true, incidentKey, event, runbook };
    }
    const response = await fetchImpl("https://events.pagerduty.com/v2/enqueue", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(event),
    });
    const ok = response.status >= 200 && response.status < 300;
    if (!ok) metrics.failures += 1; else metrics.pagerDutyEvents += 1;
    return { ok, dryRun: false, status: response.status, incidentKey, event, runbook };
  } catch (err) {
    metrics.failures += 1;
    throw err;
  } finally {
    const latencyMs = Number((Number(process.hrtime.bigint() - started) / 1e6).toFixed(3));
    metrics.totalLatencyMs += latencyMs;
    metrics.latencies.push(latencyMs);
  }
}

module.exports = { buildPagerDutyEvent, getRunbook, listRunbooks, resetIncidentMetrics, snapshotIncidentMetrics, triggerIncident };
