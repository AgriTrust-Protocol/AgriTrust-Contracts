# Runbook: Service Mesh mTLS

## Preflight

1. Confirm every pod in the `agritrust` namespace has an Istio sidecar.
2. Confirm service accounts match the principals in the authorization policies.
3. Apply `k8s/istio/agritrust-service-mesh.yaml` in a staging namespace first.
4. Run smoke tests through the ingress gateway and from an in-mesh workload.

## Incident response

- If availability drops below 99.99%, shift the `VirtualService` route to the last healthy blue subset.
- If P99 latency exceeds 100 ms, compare blue and green request histograms and disable canary headers while investigating.
- If mTLS handshakes fail, inspect workload certificates, sidecar injection status, and control-plane push errors.
- If valid traffic is denied, verify JWT scopes, source principals, HTTP methods, and paths against the active `AuthorizationPolicy` objects.

## Recovery validation

1. Confirm all `/escrow/*` critical paths return expected status codes.
2. Confirm `istio_request_duration_milliseconds` P99 is below 100 ms.
3. Confirm `istio_requests_total` success rate supports 99.99% availability.
4. Confirm no plaintext service-to-service traffic is accepted.
