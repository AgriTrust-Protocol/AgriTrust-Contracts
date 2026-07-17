# Service Mesh Integration with Mutual TLS

## Architecture

AgriTrust services run behind an Istio-compatible service mesh with sidecar proxies enforcing encrypted service-to-service traffic. The baseline manifest in `k8s/istio/agritrust-service-mesh.yaml` applies a namespace-wide `PeerAuthentication` policy with `STRICT` mutual TLS, then layers JWT request authentication and least-privilege authorization policies for the escrow API.

## Security controls

- Mutual TLS is mandatory for every workload in the `agritrust` namespace.
- A default-deny authorization policy prevents accidental lateral movement.
- Escrow API ingress is limited to the mesh ingress gateway service account, the `GET` and `POST` methods, `/escrow/*` paths, and JWT scopes for read or write operations.
- Egress from the API is constrained to the in-cluster Stellar RPC service on port 443.
- Destination rules use `ISTIO_MUTUAL`, connection pools, and outlier detection to keep critical paths below the 100 ms P99 target while isolating failing endpoints.

## Deployment strategy

Deploy mesh policy separately from application rollouts:

1. Label the namespace for sidecar injection and restart workloads.
2. Apply the service mesh baseline.
3. Deploy the green version next to the active blue version.
4. Route canary traffic with the `x-agritrust-canary: true` header, then shift the default route from 90/10 to 50/50 and finally 0/100 after SLO checks pass.
5. Roll back by restoring the blue route to 100% and scaling down green.

## Canary analysis gates

Promotion requires all of the following for at least 30 minutes:

- P99 latency for `/escrow/*` below 100 ms.
- Availability at or above 99.99%.
- No increase in `response_code=5xx` rates relative to blue.
- No authorization-denied spikes outside expected test traffic.
- mTLS certificate rotation and workload identity checks are healthy.

## Monitoring and alerting

Track these metrics from Envoy, Prometheus, and the control plane:

- `istio_request_duration_milliseconds_bucket` for P99 latency.
- `istio_requests_total` split by response code, source workload, destination workload, and route.
- `pilot_xds_push_errors` and certificate expiration metrics for control-plane health.
- Authorization denials from Istio security policy logs.

Alert when P99 exceeds 100 ms for 5 minutes, availability falls below 99.99%, any workload accepts plaintext traffic, or control-plane push errors persist.

## Security review checklist

- Confirm all workloads have injected sidecars before enforcing strict mTLS.
- Verify `PeerAuthentication` is `STRICT` in every production namespace.
- Review principal names whenever service accounts change.
- Validate JWT issuer and JWKS URLs for the target environment.
- Run canary analysis and preserve evidence for the deployment record.
