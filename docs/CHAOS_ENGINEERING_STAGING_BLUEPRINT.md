# Chaos Engineering Testing Blueprint for Staging

This blueprint defines a staging-only chaos engineering program for AgriTrust services and contracts. It is designed for system-wide exercises while preserving the performance target of **< 100 ms P99** for critical paths, the availability target of **99.99%**, and mandatory security review before each experiment is enabled.

## Objectives

- Validate that escrow APIs, Stellar adapters, contract read paths, legal-hold gates, and monitoring controls recover from realistic dependency failures.
- Detect regressions in P99 latency, error rate, and service saturation before production release.
- Exercise blue-green and canary release procedures with automated rollback gates.
- Produce auditable evidence for security and operational review.

## Staging Safety Boundaries

Chaos experiments must never run against production credentials, production RPC endpoints, or real donor/grantee funds. Every experiment requires:

1. A staging environment label and staging-only secret scope.
2. An approved experiment owner and rollback owner.
3. A blast-radius limit covering affected service, percentage of traffic, duration, and failure mode.
4. A security review confirming that injected faults do not weaken authentication, authorization, legal-hold checks, or audit logging.
5. A stop condition that halts the experiment when an SLO budget is threatened.

## Architecture

```text
+-------------------+      +----------------------+      +------------------+
| Chaos Scheduler   | ---> | Experiment Controller| ---> | Fault Injectors  |
+-------------------+      +----------------------+      +------------------+
        |                            |                            |
        v                            v                            v
+-------------------+      +----------------------+      +------------------+
| Change Calendar   |      | SLO Gate Evaluator   |      | API / Adapter /  |
| + Approvals       |      | + Rollback Hooks     |      | RPC / DB Faults  |
+-------------------+      +----------------------+      +------------------+
        |                            |                            |
        v                            v                            v
+--------------------------------------------------------------------------+
| Observability: traces, logs, metrics, synthetic probes, audit evidence   |
+--------------------------------------------------------------------------+
```

### Components

- **Chaos Scheduler:** starts approved experiments only during staging windows and blocks execution during releases or incident response.
- **Experiment Controller:** applies a signed experiment manifest, enforces duration limits, and writes immutable audit events.
- **Fault Injectors:** introduce latency, packet loss, HTTP 5xx responses, RPC timeouts, process restarts, or dependency throttling.
- **SLO Gate Evaluator:** continuously compares live telemetry to experiment thresholds and invokes rollback hooks when stop conditions are met.
- **Observability Plane:** provides dashboards, alerts, traces, logs, and post-experiment evidence.

## Experiment Manifest

Each experiment should be declared as a versioned manifest:

```yaml
apiVersion: agritrust.io/v1
kind: ChaosExperiment
metadata:
  name: escrow-read-rpc-timeout
  environment: staging
  owner: platform-oncall
spec:
  target:
    service: escrow-api
    selector: route=/escrow/:id
  fault:
    type: rpc-timeout
    dependency: stellar-rpc
    ratePercent: 10
    duration: 5m
  safeguards:
    maxP99LatencyMs: 100
    maxErrorRatePercent: 1
    minAvailabilityPercent: 99.99
    requireSecurityReview: true
  rollback:
    strategy: disable-experiment-and-shift-to-blue
    notify: [platform-oncall, security-review]
```

## Core Test Scenarios

| Scenario | Target | Fault | Expected behavior | Stop condition |
| --- | --- | --- | --- | --- |
| RPC timeout | Stellar adapter | 10% timeout for 5 minutes | Retries are bounded, errors are classified, legal-hold state is not bypassed | P99 >= 100 ms for 3 consecutive windows or error rate > 1% |
| Escrow read latency | Escrow API | Add 75 ms latency to read dependency | Cache/read fallback keeps critical path under target | P99 >= 100 ms |
| Process restart | API worker | Restart one staging worker | Health check removes unhealthy instance and traffic drains | Availability < 99.99% |
| Dependency 5xx | metadata worker | 20% upstream 5xx | Circuit breaker opens and alerts fire | Saturation > 80% or retry storm detected |
| Network packet loss | service mesh | 2% loss between API and adapter | Retries remain within budget and dashboards show impact | Error budget burn rate > 2x |

## Monitoring, Alerting, and Dashboards

Dashboards must include:

- P50/P95/P99 latency for `/escrow` routes, adapter calls, and contract read paths.
- Error rate split by user-facing HTTP status and internal dependency error class.
- Availability and synthetic probe success rate.
- Retry count, circuit-breaker state, queue depth, and worker saturation.
- Experiment metadata overlays showing start, stop, owner, fault type, and rollback result.

Alerts must page staging on-call when any stop condition is breached and must notify security review when audit logging, legal-hold checks, or authorization probes fail.

## Blue-Green and Canary Procedure

1. Deploy the candidate build to the green staging environment.
2. Run smoke tests and security checks against green with no user traffic.
3. Shift 5% synthetic and internal staging traffic to green.
4. Run low-blast-radius chaos experiments and compare blue versus green SLOs.
5. Increase canary traffic to 25%, 50%, and 100% only when P99 latency remains below 100 ms and availability remains at or above 99.99%.
6. Roll back to blue immediately when a stop condition triggers.
7. Archive dashboards, logs, traces, and manifest approvals with the release evidence.

## Runbook

### Before an Experiment

- Confirm the manifest is reviewed by platform and security.
- Verify staging secrets and RPC endpoints cannot reach production.
- Confirm dashboards and alerts are healthy.
- Announce the experiment window and rollback owner.

### During an Experiment

- Monitor SLO gates, burn rate, audit logs, and synthetic probes.
- Do not run concurrent experiments unless they are explicitly approved as a combined failure mode.
- Stop immediately when the SLO gate evaluator triggers rollback.

### After an Experiment

- Record observed impact, detection time, rollback time, and customer-facing risk.
- Create follow-up issues for missing alerts, weak recovery behavior, or documentation gaps.
- Attach evidence to the staging release record.

## Acceptance Criteria

- All staging chaos manifests define target, fault, blast radius, safeguards, rollback, owner, and security review status.
- Critical-path P99 remains below 100 ms during approved steady-state experiments.
- Staging availability remains at or above 99.99% or rollback is automatically triggered.
- Dashboards and alerts show experiment impact within one telemetry interval.
- Runbooks identify owners, stop conditions, and post-experiment evidence requirements.
