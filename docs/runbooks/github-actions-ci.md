# GitHub Actions CI Runbook

## Purpose

Use this runbook when the optimized CI workflow is slow, flaky, or blocking a protected branch.

## Triage checklist

1. Open the failed workflow run and inspect `ci-summary` to identify the upstream failed job.
2. If `node-tests` failed, run `npm ci` followed by `npm test -- --maxWorkers=50%` locally.
3. If a Rust shard failed, copy the package list from the matrix entry and run `cargo test -p <package> --locked` for each affected package.
4. If `security-review` failed, review `npm audit --audit-level=high` output and Cargo lockfile drift.
5. If `build-wasm` failed, run `cargo build --target wasm32-unknown-unknown --release --locked` locally.

## Slow workflow response

- Check whether a newer commit cancelled older runs as expected.
- Review npm and Cargo cache-hit logs.
- Compare job duration by shard; rebalance packages if one shard consistently dominates total runtime.
- Re-run only failed jobs first; re-run the entire workflow only when cache corruption or runner failure is suspected.

## Branch protection

Require `CI summary gate` for protected branches. Individual shard names can change as the workspace evolves, but the summary gate is the stable required status.

## Security escalation

Treat `security-review` failures as release-blocking. Do not bypass the summary gate until the dependency advisory or lockfile issue is reviewed and documented by the security owner.
