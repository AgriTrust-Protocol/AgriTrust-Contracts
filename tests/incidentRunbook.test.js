"use strict";

const request = require("supertest");
const app = require("../src/index");
const {
  buildPagerDutyEvent,
  getRunbook,
  resetIncidentMetrics,
  snapshotIncidentMetrics,
  triggerIncident,
} = require("../src/services/incidentRunbook");

describe("incident runbook automation", () => {
  beforeEach(resetIncidentMetrics);

  it("lists known operational runbooks", async () => {
    const res = await request(app).get("/incidents/runbooks");
    expect(res.status).toBe(200);
    expect(res.body.runbooks.map((runbook) => runbook.id)).toContain("postgres_pool_exhaustion");
  });

  it("builds PagerDuty Events API payloads with runbook context", () => {
    const runbook = getRunbook("service_mesh_mtls_failure");
    const event = buildPagerDutyEvent({ runbook, incidentKey: "mesh-1", severity: "critical" });
    expect(event).toMatchObject({ event_action: "trigger", dedup_key: "mesh-1" });
    expect(event.payload.custom_details.steps).toHaveLength(3);
  });

  it("triggers PagerDuty through an injectable fetch client", async () => {
    const fetchImpl = jest.fn().mockResolvedValue({ status: 202 });
    const result = await triggerIncident({ runbookId: "webhook_delivery_degradation", incidentKey: "webhook-1" }, { fetchImpl });
    expect(result.ok).toBe(true);
    expect(fetchImpl).toHaveBeenCalledWith("https://events.pagerduty.com/v2/enqueue", expect.objectContaining({ method: "POST" }));
    expect(snapshotIncidentMetrics()).toMatchObject({ triggered: 1, pagerDutyEvents: 1, failures: 0 });
  });

  it("supports dry-run incident triggers over HTTP", async () => {
    const res = await request(app)
      .post("/incidents/trigger?dryRun=true")
      .send({ runbookId: "postgres_pool_exhaustion", severity: "critical", incidentKey: "pg-1" });
    expect(res.status).toBe(202);
    expect(res.body.dryRun).toBe(true);
    expect(res.body.event.payload.custom_details.runbook).toBe("/docs/runbooks/postgres-pool-health.md");
  });

  it("rejects unknown runbooks", async () => {
    const res = await request(app).post("/incidents/trigger?dryRun=true").send({ runbookId: "missing" });
    expect(res.status).toBe(404);
  });
});
