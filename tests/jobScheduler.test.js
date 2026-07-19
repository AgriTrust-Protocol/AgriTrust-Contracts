"use strict";

const request = require("supertest");
const app = require("../src/index");
const { LeaseJobScheduler } = require("../src/services/jobScheduler");

describe("LeaseJobScheduler", () => {
  let now;
  let scheduler;

  beforeEach(() => {
    now = 1_000;
    scheduler = new LeaseJobScheduler({ now: () => now, leaseMs: 100, tokenFactory: () => `lease-${now}` });
  });

  it("claims ready jobs by priority and creation time with a fencing token", () => {
    scheduler.enqueue({ id: "low", priority: 1 });
    now += 1;
    scheduler.enqueue({ id: "high", priority: 5 });

    const claimed = scheduler.claimNext({ workerId: "worker-a" });
    expect(claimed).toMatchObject({ id: "high", status: "leased", leaseOwner: "worker-a", leaseToken: "lease-1001", attempts: 1 });
    expect(claimed.leaseExpiresAt).toBe(1101);
  });

  it("prevents a second worker from completing a job without the active lease token", () => {
    scheduler.enqueue({ id: "job-1" });
    const claimed = scheduler.claimNext({ workerId: "worker-a" });

    expect(() => scheduler.complete({ jobId: "job-1", workerId: "worker-b", leaseToken: claimed.leaseToken })).toThrow("Lease token");
    expect(scheduler.snapshotMetrics().lease_conflicts_total).toBe(1);
  });

  it("reclaims expired leases and lets another worker claim the job", () => {
    scheduler.enqueue({ id: "job-1" });
    scheduler.claimNext({ workerId: "worker-a" });
    now += 101;

    const reclaimed = scheduler.claimNext({ workerId: "worker-b" });
    expect(reclaimed).toMatchObject({ id: "job-1", leaseOwner: "worker-b", attempts: 2 });
    expect(scheduler.snapshotMetrics().expired_leases_reclaimed_total).toBe(1);
  });

  it("renews and completes only with the owner lease", () => {
    scheduler.enqueue({ id: "job-1" });
    const claimed = scheduler.claimNext({ workerId: "worker-a" });
    now += 50;
    const renewed = scheduler.renewLease({ jobId: "job-1", workerId: "worker-a", leaseToken: claimed.leaseToken, leaseMs: 200 });
    expect(renewed.leaseExpiresAt).toBe(1250);

    const completed = scheduler.complete({ jobId: "job-1", workerId: "worker-a", leaseToken: claimed.leaseToken });
    expect(completed.status).toBe("completed");
    expect(scheduler.snapshotMetrics()).toMatchObject({ completed_total: 1, jobs: { completed: 1 } });
  });

  it("emits P99 and Prometheus metrics for dashboard alerting", () => {
    scheduler.enqueue({ id: "job-1" });
    scheduler.claimNext({ workerId: "worker-a" });
    expect(scheduler.snapshotMetrics()).toHaveProperty("p99_critical_path_latency_ms");
    expect(scheduler.prometheusMetrics()).toContain("agritrust_scheduler_target_p99_ms 100");
  });
});

describe("jobs routes", () => {
  it("enqueues, claims, completes, and exposes metrics", async () => {
    const id = `route-job-${Date.now()}`;
    await expect(request(app).post("/jobs").send({ id, priority: 3 })).resolves.toMatchObject({ status: 202 });
    const claim = await request(app).post("/jobs/claim").send({ workerId: "route-worker" });
    expect(claim.status).toBe(200);
    expect(claim.body).toMatchObject({ id, status: "leased", leaseOwner: "route-worker" });

    const complete = await request(app).post(`/jobs/${id}/complete`).send({ workerId: "route-worker", leaseToken: claim.body.leaseToken });
    expect(complete.status).toBe(200);
    expect(complete.body.status).toBe("completed");

    const metrics = await request(app).get("/jobs/metrics");
    expect(metrics.status).toBe(200);
    expect(metrics.body).toHaveProperty("p99_critical_path_latency_ms");
  });
});
