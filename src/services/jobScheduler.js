"use strict";

const crypto = require("crypto");

const DEFAULT_LEASE_MS = 30_000;
const DEFAULT_MAX_ATTEMPTS = 5;
const DEFAULT_TARGET_P99_MS = 100;
const JOB_ID_PATTERN = /^[A-Za-z0-9_.:-]+$/;

function percentile(values, p) {
  if (!values.length) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.ceil(sorted.length * p) - 1)];
}

function assertSafeId(value, label) {
  if (typeof value !== "string" || !value.trim() || value.length > 128 || !JOB_ID_PATTERN.test(value)) {
    const err = new Error(`${label} must be 1-128 URL-safe characters`);
    err.statusCode = 400;
    throw err;
  }
  return value.trim();
}

function cloneJob(job) {
  return {
    id: job.id,
    queue: job.queue,
    status: job.status,
    payload: job.payload,
    priority: job.priority,
    runAt: job.runAt,
    attempts: job.attempts,
    maxAttempts: job.maxAttempts,
    leaseOwner: job.leaseOwner,
    leaseToken: job.leaseToken,
    leaseExpiresAt: job.leaseExpiresAt,
    createdAt: job.createdAt,
    updatedAt: job.updatedAt,
    completedAt: job.completedAt,
    failedReason: job.failedReason,
  };
}

class LeaseJobScheduler {
  constructor({ now = Date.now, leaseMs = DEFAULT_LEASE_MS, targetP99Ms = DEFAULT_TARGET_P99_MS, tokenFactory } = {}) {
    this.now = now;
    this.leaseMs = leaseMs;
    this.targetP99Ms = targetP99Ms;
    this.tokenFactory = tokenFactory || (() => crypto.randomUUID());
    this.jobs = new Map();
    this.metrics = {
      enqueued_total: 0,
      claimed_total: 0,
      completed_total: 0,
      failed_total: 0,
      lease_renewed_total: 0,
      lease_conflicts_total: 0,
      expired_leases_reclaimed_total: 0,
      critical_path_latency_ms: [],
    };
  }

  recordLatency(startedAt) {
    this.metrics.critical_path_latency_ms.push(Math.max(0, this.now() - startedAt));
    if (this.metrics.critical_path_latency_ms.length > 1024) this.metrics.critical_path_latency_ms.shift();
  }

  enqueue({ id, queue = "default", payload = {}, priority = 0, runAt, maxAttempts = DEFAULT_MAX_ATTEMPTS }) {
    const startedAt = this.now();
    const jobId = assertSafeId(id, "job id");
    if (this.jobs.has(jobId)) {
      const err = new Error("Job already exists");
      err.statusCode = 409;
      throw err;
    }
    const timestamp = this.now();
    const job = {
      id: jobId,
      queue: assertSafeId(queue, "queue"),
      payload,
      priority: Number(priority) || 0,
      runAt: runAt ? new Date(runAt).getTime() : timestamp,
      maxAttempts: Number(maxAttempts) || DEFAULT_MAX_ATTEMPTS,
      attempts: 0,
      status: "queued",
      leaseOwner: null,
      leaseToken: null,
      leaseExpiresAt: null,
      createdAt: timestamp,
      updatedAt: timestamp,
      completedAt: null,
      failedReason: null,
    };
    if (!Number.isFinite(job.runAt)) {
      const err = new Error("runAt must be a valid timestamp");
      err.statusCode = 400;
      throw err;
    }
    this.jobs.set(job.id, job);
    this.metrics.enqueued_total += 1;
    this.recordLatency(startedAt);
    return cloneJob(job);
  }

  reclaimExpired(job, nowMs) {
    if (job.status === "leased" && job.leaseExpiresAt <= nowMs) {
      job.status = job.attempts >= job.maxAttempts ? "failed" : "queued";
      job.failedReason = job.status === "failed" ? "max attempts exhausted" : null;
      job.leaseOwner = null;
      job.leaseToken = null;
      job.leaseExpiresAt = null;
      job.updatedAt = nowMs;
      this.metrics.expired_leases_reclaimed_total += 1;
      if (job.status === "failed") this.metrics.failed_total += 1;
    }
  }

  claimNext({ workerId, queue = "default", leaseMs = this.leaseMs } = {}) {
    const startedAt = this.now();
    const owner = assertSafeId(workerId, "worker id");
    const wantedQueue = assertSafeId(queue, "queue");
    const nowMs = this.now();
    let candidate = null;
    for (const job of this.jobs.values()) {
      this.reclaimExpired(job, nowMs);
      if (job.status !== "queued" || job.queue !== wantedQueue || job.runAt > nowMs) continue;
      if (!candidate || job.priority > candidate.priority || (job.priority === candidate.priority && job.createdAt < candidate.createdAt)) {
        candidate = job;
      }
    }
    if (!candidate) {
      this.recordLatency(startedAt);
      return null;
    }
    candidate.status = "leased";
    candidate.leaseOwner = owner;
    candidate.leaseToken = this.tokenFactory();
    candidate.leaseExpiresAt = nowMs + leaseMs;
    candidate.attempts += 1;
    candidate.updatedAt = nowMs;
    this.metrics.claimed_total += 1;
    this.recordLatency(startedAt);
    return cloneJob(candidate);
  }

  assertLease(jobId, workerId, leaseToken) {
    const job = this.jobs.get(assertSafeId(jobId, "job id"));
    const nowMs = this.now();
    if (!job) {
      const err = new Error("Job not found"); err.statusCode = 404; throw err;
    }
    this.reclaimExpired(job, nowMs);
    if (job.status !== "leased" || job.leaseOwner !== workerId || job.leaseToken !== leaseToken) {
      this.metrics.lease_conflicts_total += 1;
      const err = new Error("Lease token does not own this job"); err.statusCode = 409; throw err;
    }
    return job;
  }

  renewLease({ jobId, workerId, leaseToken, leaseMs = this.leaseMs }) {
    const startedAt = this.now();
    const job = this.assertLease(jobId, assertSafeId(workerId, "worker id"), leaseToken);
    job.leaseExpiresAt = this.now() + leaseMs;
    job.updatedAt = this.now();
    this.metrics.lease_renewed_total += 1;
    this.recordLatency(startedAt);
    return cloneJob(job);
  }

  complete({ jobId, workerId, leaseToken }) {
    const startedAt = this.now();
    const job = this.assertLease(jobId, assertSafeId(workerId, "worker id"), leaseToken);
    job.status = "completed";
    job.completedAt = this.now();
    job.updatedAt = job.completedAt;
    job.leaseOwner = null;
    job.leaseToken = null;
    job.leaseExpiresAt = null;
    this.metrics.completed_total += 1;
    this.recordLatency(startedAt);
    return cloneJob(job);
  }

  snapshotMetrics() {
    const counts = { queued: 0, leased: 0, completed: 0, failed: 0 };
    const nowMs = this.now();
    for (const job of this.jobs.values()) {
      this.reclaimExpired(job, nowMs);
      counts[job.status] += 1;
    }
    const p99 = percentile(this.metrics.critical_path_latency_ms, 0.99);
    return { ...this.metrics, p99_critical_path_latency_ms: p99, target_p99_ms: this.targetP99Ms, jobs: counts };
  }

  prometheusMetrics() {
    const snapshot = this.snapshotMetrics();
    return [
      `agritrust_scheduler_enqueued_total ${snapshot.enqueued_total}`,
      `agritrust_scheduler_claimed_total ${snapshot.claimed_total}`,
      `agritrust_scheduler_completed_total ${snapshot.completed_total}`,
      `agritrust_scheduler_failed_total ${snapshot.failed_total}`,
      `agritrust_scheduler_lease_conflicts_total ${snapshot.lease_conflicts_total}`,
      `agritrust_scheduler_p99_critical_path_latency_ms ${snapshot.p99_critical_path_latency_ms}`,
      `agritrust_scheduler_target_p99_ms ${snapshot.target_p99_ms}`,
    ].join("\n");
  }
}

module.exports = { LeaseJobScheduler, DEFAULT_LEASE_MS, DEFAULT_MAX_ATTEMPTS };
