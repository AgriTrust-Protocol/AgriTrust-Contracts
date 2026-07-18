# Secret Rotation Service for Database Credentials and API Keys

## Architecture

The service centralises rotation for database credentials and partner API keys. It uses an injected secret provider so production deployments can bind to Vault, AWS Secrets Manager, or another KMS-backed store while local tests use `InMemorySecretProvider`.

Core flow:

1. Inventory registered secrets with a name, type, value, version, and `rotatedAt` timestamp.
2. Detect due secrets from the policy interval or missing rotation metadata.
3. Generate a replacement value with type-specific prefixes.
4. Promote the replacement atomically through the provider.
5. Keep only fingerprints in logs and API responses.
6. Expose metrics for dashboards and alerts.

## Performance and Availability

Critical reads are measured in `SecretRotationService.getSecret()`. The default P99 target is 100 ms and slow reads emit a `secret_read_slow` warning for alerting. Rotation is designed for blue-green deployment: old and new versions can coexist for the configured grace period, allowing canary validation before revoking previous credentials.

## Monitoring and Alerting

`GET /internal/secrets/metrics` returns rotation policy settings for scraping. Production dashboards should include:

- rotation success/failure count by secret type;
- secrets past due for rotation;
- critical-path read latency P50/P95/P99;
- failed promotion attempts;
- canary health during blue-green rollout.

## Runbook

1. Deploy the new rotation service to the green environment.
2. Run canary rotation for a non-critical API key.
3. Confirm metrics stay under the 100 ms P99 target and no authentication errors increase.
4. Promote database credentials, keeping the previous fingerprint valid until the grace period expires.
5. Shift traffic to green after canary analysis passes.
6. Revoke expired credentials and archive the rotation event for security review.

## Security Notes

- Never log raw secret values.
- Return redacted secret metadata from operational endpoints.
- Store providers must enforce least-privilege read/write policies.
- All rotations should be reviewed as part of the security release checklist.
