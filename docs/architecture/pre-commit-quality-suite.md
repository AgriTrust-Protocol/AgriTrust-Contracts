# Pre-Commit Quality Suite Architecture

## Goals

The pre-commit quality suite provides a repository-wide first line of defense before code reaches CI. It is intentionally lightweight so the critical local path stays below the 100 ms P99 target for normal staged changes while still blocking common quality and security regressions.

## Components

- `.githooks/pre-commit` is the Git entrypoint. It delegates to the Node.js checker and fails closed on any non-zero exit.
- `scripts/pre-commit-quality.js` discovers staged added, copied, modified, and renamed files via `git diff --cached --name-only --diff-filter=ACMR`.
- `checkFile` skips binary files and files larger than 1 MiB to keep latency predictable, then enforces text hygiene and high-confidence secret patterns.
- `npm run precommit:install` configures `core.hooksPath` so contributors can opt in with a single command.
- `npm run quality:precommit` runs the same checks directly for CI jobs or manual validation.

## Enforced Controls

1. Text files must end with a trailing newline.
2. CRLF line endings are rejected to keep generated diffs stable across platforms.
3. Trailing whitespace is rejected with line-level diagnostics.
4. High-confidence credentials such as AWS access keys, private-key blocks, GitHub tokens, and Slack tokens are blocked before commit.

## Monitoring and Alerting

Local hooks do not emit telemetry, but the same `npm run quality:precommit` command can be wired into CI. Recommended CI signals are:

- hook pass/fail counts per branch;
- median and P99 hook runtime;
- blocked-secret event count;
- skipped-file count for files over the 1 MiB latency guardrail.

Alert when the CI P99 runtime exceeds 100 ms for three consecutive runs or when any blocked-secret event occurs on a protected branch.

## Deployment Strategy

Use a two-phase rollout:

1. Canary: enable `npm run quality:precommit` as a non-blocking CI step and ask one service team to run `npm run precommit:install` locally.
2. Blue-green cutover: make the CI step blocking after the canary period, then document the hook installation in onboarding material.

Rollback is safe: remove the CI invocation or reset local Git hooks with `git config --unset core.hooksPath`.

## Security Review Notes

The suite uses only Node.js built-ins and Git, so it does not expand the dependency attack surface. Secret patterns are deliberately high-confidence to minimize false positives while preventing accidental credential commits.
