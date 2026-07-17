# Redis Cache Layer

AgriTrust API reads now pass through a cache-aside layer before calling the on-chain adapter. The cache targets high-volume read paths, starting with `readEscrow`, so repeated requests can stay under the 100 ms P99 target when Redis is colocated with the API.

## Architecture

1. Request handlers call service functions such as `readEscrow`.
2. Services validate inputs before cache lookup to prevent cache-key abuse.
3. The cache layer builds namespaced keys (`agritrust:escrow:<escrowId>` by default).
4. Cache hits return the normalized object immediately.
5. Cache misses call the on-chain adapter, serialize the response, and store it with a TTL.
6. Cache errors are fail-open for read availability: the API records the error and falls back to the on-chain read path.

Redis is enabled when `REDIS_URL` is set. The built-in Redis store uses the Redis RESP protocol over TCP, so the runtime does not need an extra Node package. Local and test environments use an in-memory TTL store with the same cache-aside behavior.

## Configuration

| Variable | Default | Description |
| --- | --- | --- |
| `CACHE_ENABLED` | `true` | Set to `false` to bypass cache lookups and writes. |
| `CACHE_TTL_SECONDS` | `60` | Integer TTL from 1 to 86,400 seconds. |
| `CACHE_KEY_PREFIX` | `agritrust` | Prefix used for all cache keys. |
| `REDIS_URL` | unset | Redis connection URL. When unset, in-memory cache is used. |

## Monitoring and alerting

`GET /health/cache` returns counters for `hits`, `misses`, `sets`, and `errors`, plus the active TTL and enabled flag. Production dashboards should graph cache hit ratio, cache error count, Redis latency, Redis memory utilization, and on-chain adapter latency. Alert when cache errors increase for 5 minutes or when hit ratio drops sharply during steady traffic.

## Deployment runbook

1. Deploy Redis and verify TLS/auth configuration outside the API container.
2. Ship the API with `CACHE_ENABLED=false` for the blue environment.
3. Enable cache for a 5% canary by setting `CACHE_ENABLED=true`, `REDIS_URL`, and a conservative `CACHE_TTL_SECONDS`.
4. Compare canary P99 latency, 5xx rate, cache errors, and on-chain adapter call volume against blue.
5. Increase traffic in stages when metrics stay healthy.
6. Roll back by setting `CACHE_ENABLED=false`; no schema migration is required.

## Security notes

Escrow IDs are validated before cache-key construction. Cache failures do not expose internal errors to clients. Keep Redis private to the service network, require authentication, and avoid storing secrets in cached payloads.
