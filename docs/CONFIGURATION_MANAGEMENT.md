# Configuration Management

AgriTrust services load runtime configuration from `config/runtime.json` or the path named by `AGRITRUST_CONFIG_PATH`.
The API validates configuration against an explicit schema before it is made active. Invalid reloads are rejected and the last known-good configuration remains in use.

## Architecture

- `ConfigManager` owns the active immutable configuration snapshot.
- File watching provides hot-reload with a short debounce to avoid partial reads while files are being written.
- Manual reloads are available through `POST /ops/config/reload` for blue-green or canary rollouts.
- Read-only visibility is available through `GET /ops/config/runtime`, including reload counters for monitoring and alerting.

## Schema

```json
{
  "rateLimit": { "capacity": 60, "refillPerSecond": 30 },
  "features": { "shedCapacity": false, "mutationEndpoints": true, "escrowRead": true },
  "capacity": { "maxInFlight": null },
  "observability": { "hotReloadEnabled": true }
}
```

Numeric values must be positive. Feature and observability flags must be booleans. `capacity.maxInFlight` may be `null` to disable capacity shedding limits.

## Operations

1. Commit a candidate config and deploy it to the green environment.
2. Call `POST /ops/config/reload` on green instances.
3. Watch `GET /ops/config/runtime` for `reloadFailures`, `validationFailures`, and `lastError`.
4. Shift canary traffic only after reload success counters advance and no validation errors are present.
5. If reload fails, fix the candidate file; the service keeps using the last valid snapshot.
