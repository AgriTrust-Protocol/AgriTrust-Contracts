# API Rate Limiting with Per-Tenant Token Buckets

AgriTrust API traffic is protected by a system-wide per-tenant token bucket
middleware registered before service routes. Each tenant receives an independent
bucket keyed by the `x-tenant-id` header; requests without the header are grouped
under `anonymous`.

## Architecture

1. `src/index.js` installs `createTenantRateLimiter()` immediately after JSON
   parsing and before `/escrow` routes, so all current API endpoints share the
   same enforcement path.
2. `src/middleware/tenantRateLimiter.js` performs O(1) lookup and refill using a
   `Map` of tenant IDs to token buckets. Each request consumes one token.
3. On success, the middleware forwards to the route and emits
   `X-RateLimit-Limit`, `X-RateLimit-Remaining`, and `X-RateLimit-Tenant`.
4. When a tenant exhausts its bucket, the middleware returns HTTP `429` with a
   `Retry-After` header.

## Defaults and tuning

| Setting | Environment variable | Default |
| --- | --- | --- |
| Bucket capacity | `RATE_LIMIT_CAPACITY` | `60` |
| Refill rate | `RATE_LIMIT_REFILL_PER_SECOND` | `30` |

Production deployments should externalize buckets to a low-latency shared store
when running more than one API replica. The middleware's snapshot hook is
intended for metrics export: emit allowed/blocked counts, remaining tokens, and
429 rates per tenant into the existing monitoring backend.

## Operations

* Alert when tenant-level `429` rates spike above the canary baseline.
* During blue-green deploys, mirror traffic to the green stack and compare P99
  latency plus 429 rates before promoting.
* If a tenant is incorrectly throttled, temporarily increase
  `RATE_LIMIT_CAPACITY` and `RATE_LIMIT_REFILL_PER_SECOND` for the affected
  environment, then review tenant attribution.
