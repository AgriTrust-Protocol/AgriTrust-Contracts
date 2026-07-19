# Docker Layer Cache CI Runbook

## Objective

The Docker layer cache workflow keeps CI image builds fast and reproducible across the API and smart-contract artifact builders. It uses Docker BuildKit cache mounts inside Dockerfiles and the GitHub Actions cache backend for cross-run layer reuse.

## Architecture

- `.github/workflows/docker-layer-cache.yml` builds every service image through a matrix job.
- `.dockerignore` removes volatile and heavyweight directories such as `target/`, `node_modules/`, and `.git/` from Docker build contexts.
- `docker/api.Dockerfile` copies package manifests before source files so dependency layers are invalidated only when Node dependencies change.
- `docker/contracts.Dockerfile` fetches Rust dependencies before the release build and uses BuildKit cache mounts for Cargo registry, git, and target directories.

## Monitoring and alerting

Review the `Docker layer cache` workflow duration and the Buildx cache import/export lines in each run. Alert the platform channel when either condition holds:

1. The same service has three consecutive build durations more than 50% above its 14-day median.
2. Buildx reports cache import failures or cache export failures for `type=gha`.
3. The workflow blocks a release branch for more than 15 minutes.

## Deployment strategy

This workflow is build-only for pull requests. For release branches, publish images from the validated digest using the existing blue-green deployment pipeline, then promote traffic through canary analysis before full rollout.

## Security review checklist

- Keep Dockerfiles pinned to maintained base-image major versions.
- Do not pass secrets as Docker build arguments.
- Verify `.dockerignore` excludes local environment files and build outputs.
- Preserve `npm ci` and `cargo --locked` semantics so dependency resolution remains deterministic.

## Troubleshooting

- If dependency layers rebuild unexpectedly, compare changes to `package-lock.json`, `Cargo.lock`, and service manifests.
- If cache export fails, rerun the job once; GitHub cache backend failures are often transient.
- If Rust builds run out of space, reduce cache scope by service or clear old GitHub Actions caches.
