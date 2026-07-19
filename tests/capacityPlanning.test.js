"use strict";

const request = require("supertest");
const { CapacityPlanner } = require("../src/services/capacityPlanning");
const app = require("../src/index");

describe("CapacityPlanner", () => {
  it("rolls historical usage into forecasts and capacity recommendations", () => {
    let nowMs = Date.parse("2026-07-18T00:00:00.000Z");
    const planner = new CapacityPlanner({ now: () => nowMs, bucketMs: 60_000, windowMs: 3_600_000 });

    for (let i = 0; i < 8; i += 1) planner.recordUsage({ service: "escrow", latencyMs: 95, timestampMs: nowMs });
    nowMs += 60_000;
    for (let i = 0; i < 10; i += 1) planner.recordUsage({ service: "escrow", latencyMs: 125, statusCode: i === 0 ? 500 : 200, timestampMs: nowMs });

    const trend = planner.trend("escrow", { capacityPerMinute: 10 });

    expect(trend.total_requests).toBe(18);
    expect(trend.peak_rpm).toBe(10);
    expect(trend.projected_next_hour_requests).toBe(540);
    expect(trend.recommended_capacity_per_minute).toBe(13);
    expect(trend.alerts.map((alert) => alert.code)).toEqual([
      "capacity_utilization_high",
      "p99_latency_target_breached",
    ]);
  });

  it("exports Prometheus metrics for dashboards and alerts", () => {
    const planner = new CapacityPlanner({ now: () => 0 });
    planner.recordUsage({ service: "api", latencyMs: 25, timestampMs: 0 });

    expect(planner.prometheusMetrics()).toContain('agritrust_capacity_peak_rpm{service="api"} 1');
  });
});

describe("capacity planning routes", () => {
  it("publishes historical capacity trends from the health API", async () => {
    await request(app).get("/healthz").expect(200);
    const res = await request(app).get("/health/capacity").expect(200);

    expect(res.body).toHaveProperty("generated_at");
    expect(res.body.services.length).toBeGreaterThan(0);
  });
});
