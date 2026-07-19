# Coverage Threshold CI Runbook

## Purpose

The coverage gate prevents changes from merging unless the JavaScript API test
suite satisfies the repository-wide Jest thresholds configured in `package.json`.
The gate runs on pull requests and on pushes to the default branches so coverage
regressions are detected before deployment.

## Architecture

1. GitHub Actions checks out the repository and installs Node.js dependencies
   with `npm ci`.
2. `npm run ci:coverage`
   executes Jest with coverage enabled.
3. Jest enforces the global statement, branch, function, and line thresholds
   from `package.json` and exits non-zero when any threshold is missed.
4. The workflow uploads the generated `coverage/` directory as an artifact for
   inspection whether the gate passes or fails.

## Monitoring and alerting

GitHub branch protection should require the `JavaScript coverage threshold` job
before merge. Failed workflow runs are the alert source for coverage regressions;
owners should subscribe to repository Actions notifications and review the lcov
artifact to identify untested files.

## Remediation

1. Open the failed Actions run and download the `jest-coverage` artifact.
2. Review the text summary and lcov HTML output for the lowest-covered files.
3. Add or update tests for changed behavior instead of lowering thresholds.
4. Re-run `npm run test:coverage -- --coverageReporters=text-summary` locally.
5. Push the fix and wait for the CI coverage gate to pass.

## Deployment notes

The coverage gate is a CI-only control and does not affect runtime critical
paths or availability. Enable the workflow with branch protection first, then
observe it for at least one pull request before making it a required production
release check.
