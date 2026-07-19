# Runtime Configuration Auditing and Drift Detection

AgriTrust services expose a lightweight runtime configuration auditor for detecting environment drift without adding latency to escrow critical paths. The auditor compares a bounded, non-secret runtime configuration contract against a boot-time baseline and returns deterministic SHA-256 fingerprints for audit trails.

## Architecture

- `RuntimeConfigAuditor` stores a canonical baseline, computes current fingerprints, and records audit events in memory.
- `/ops/config/snapshot` performs an on-demand audit and returns `200` when configuration matches or `409` when drift is detected.
- `/ops/config/metrics` exposes counters that can be scraped by monitoring jobs and alert rules.
- Sensitive fields matching secret, password, token, key, or credential are redacted before responses are emitted.

## Operational guidance

1. Capture a baseline during blue-green deployment startup.
2. Canary the new environment and poll `/ops/config/snapshot` before shifting traffic.
3. Alert when `config_drift_active` is `1` or `config_drift_detected_total` increases unexpectedly.
4. Investigate drift by comparing reported keys with the intended deployment manifest, then roll forward or roll back.

The audit path is intentionally outside escrow mutation handlers, preserving the sub-100ms P99 target for critical paths.
