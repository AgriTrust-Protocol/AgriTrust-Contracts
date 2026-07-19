# Pre-Commit Quality Suite Runbook

## Install Locally

```sh
npm run precommit:install
```

## Run Manually

```sh
npm run quality:precommit
```

The checker only evaluates files staged in Git. Stage intended changes before running it.

## Triage Failures

- `missing trailing newline`: add a final newline and stage the file again.
- `contains CRLF line endings`: convert the file to LF endings.
- `trailing whitespace on line N`: remove the reported trailing spaces or tabs.
- `possible secret detected`: remove the credential, rotate it if it was real, and follow the security incident process before retrying.

## CI Operations

Add `npm run quality:precommit` before longer test jobs to fail fast. If CI alerts on P99 latency above 100 ms, inspect the staged file mix and check whether generated files should be excluded or kept out of commits.

## Rollback

For local rollback, run:

```sh
git config --unset core.hooksPath
```

For CI rollback, remove or mark the `npm run quality:precommit` step as non-blocking while the issue is investigated.
