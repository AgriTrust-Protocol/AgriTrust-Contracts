# Graceful Degradation with Feature Flags and Capacity Shedding

## Architecture

The API now has an in-process degradation control plane that is intentionally
small, deterministic, and fail-closed for critical escrow operations:

1. **Feature flags** are evaluated before route handlers and before the
   legal-hold adapter call. Operators can disable escrow reads or all escrow
   mutations without redeploying the service.
2. **Capacity shedding** runs immediately after JSON parsing. When enabled and
   the in-flight request ceiling is reached, the API returns `503` with a
   `Retry-After: 1` header instead of allowing queues to grow unbounded.
3. **Observability snapshots** are exposed at `GET /ops/degradation` for
   dashboards and alerting. The endpoint reports active flags, capacity state,
   and counters for accepted, completed, shed, and feature-disabled requests.
4. **Critical liveness** remains available through `GET /healthz`; both health
   and degradation telemetry bypass capacity shedding so load balancers and
   operators can still inspect the service during incidents.

## Configuration

| Setting | Default | Description |
| --- | --- | --- |
| `FEATURE_ESCROW_READ` | `true` | Enables `GET /escrow/:escrowId`. |
| `FEATURE_ESCROW_MUTATIONS` | `true` | Enables `fund`, `release`, and `withdraw` routes. |
| `FEATURE_SHED_CAPACITY` | `false` | Turns in-flight capacity shedding on or off. |
| `CAPACITY_SHED_MAX_IN_FLIGHT` | unlimited | Maximum concurrent non-ops requests accepted before shedding. |
| `FEATURE_FLAGS` | unset | Optional JSON object supporting `escrowRead`, `mutationEndpoints`, and `shedCapacity`. Explicit env vars take precedence. |

Boolean values accept `true/false`, `1/0`, `yes/no`, `on/off`, and
`enabled/disabled`.

## Operating Model

### Blue-green and canary rollout

1. Deploy with all flags at their safe defaults.
2. Enable `FEATURE_SHED_CAPACITY=true` on the green environment with a high
   `CAPACITY_SHED_MAX_IN_FLIGHT` value.
3. Route 5% canary traffic to green and compare P99 latency, 5xx rate, and
   `/ops/degradation` counters against blue.
4. Lower the capacity ceiling only when P99 approaches the 100ms target or when
   upstream dependencies start queuing.
5. Promote green once error budgets and degradation counters remain stable for
   the canary window.

### Alerts

Create alerts for the following telemetry conditions:

- `counters.shed > 0` for five consecutive minutes.
- `counters.disabled > 0` outside a planned change window.
- `capacity.in_flight / capacity.max_in_flight > 0.8` for five minutes.
- Any sustained API P99 latency above 100ms on escrow read or mutation paths.

### Runbook

1. Check `GET /healthz` to verify the process is alive.
2. Check `GET /ops/degradation` for active flags and shed/disabled counters.
3. If dependency latency is rising, enable `FEATURE_SHED_CAPACITY=true` and set a
   conservative `CAPACITY_SHED_MAX_IN_FLIGHT` value.
4. If mutations are unsafe, set `FEATURE_ESCROW_MUTATIONS=false`; reads can stay
   online for customer visibility.
5. If reads are returning bad data, set `FEATURE_ESCROW_READ=false` and keep
   `/healthz` plus `/ops/degradation` available for operators.
6. Re-enable features gradually through blue-green or canary traffic shifts, and
   confirm counters stop increasing before closing the incident.
