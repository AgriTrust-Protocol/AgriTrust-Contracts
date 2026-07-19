# GitHub Actions Workflow Optimization Architecture

## Goals

The optimized CI workflow keeps pull-request feedback fast while preserving the security and release checks required for AgriTrust contracts and API services.

- Run independent Node.js, Rust, security, and release-build checks in parallel.
- Cancel superseded runs for the same branch to reduce queue pressure.
- Cache dependency and compiler outputs for repeatable, lower-latency runs.
- Keep a single summary gate that branch protection can require.

## Workflow design

The workflow in `.github/workflows/ci.yml` is split into five jobs:

1. `node-tests` installs JavaScript dependencies with `npm ci` and runs Jest.
2. `rust-tests` shards workspace package tests across three matrix entries so independent contract groups run concurrently.
3. `security-review` runs `npm audit --audit-level=high` and verifies Cargo lockfile consistency.
4. `build-wasm` waits for Rust tests and security review, then builds optimized WASM artifacts.
5. `ci-summary` depends on every required job and fails if any dependency did not succeed.

This topology prioritizes early failure for unit tests and dependency security while avoiding unnecessary release builds when upstream gates are broken.

## Performance strategy

- `concurrency.cancel-in-progress` stops stale commits from consuming runners.
- `actions/setup-node` uses the npm cache keyed by `package-lock.json`.
- `Swatinem/rust-cache` caches Cargo registry, git, and build directories per shard.
- Rust tests use a package matrix instead of one serialized workspace test.
- Jest uses bounded workers to avoid CPU oversubscription on shared runners.

## Availability and deployment expectations

CI does not deploy directly. Production rollout should remain a separate, environment-protected workflow that consumes a passing commit from the summary gate and performs blue-green deployment with canary analysis before traffic promotion. Required checks should include `ci-summary` so protected branches only accept commits whose parallel jobs passed.

## Monitoring and alerting

Monitor these workflow signals from GitHub Actions metrics or exported workflow events:

- Queue time and total duration by job.
- Cache hit rates for npm and Cargo caches.
- Failure rate by shard and test command.
- Security-audit failures.
- Cancelled runs caused by superseded pushes.

Alert when the summary gate failure rate or workflow duration exceeds the service team's agreed threshold for two consecutive runs.
