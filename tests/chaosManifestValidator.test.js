const { spawnSync } = require('child_process');
const path = require('path');

const validator = path.join(__dirname, '..', 'scripts', 'chaos', 'validate-chaos-manifest.js');

function runValidator(fixture) {
  return spawnSync(process.execPath, [validator, path.join(__dirname, 'fixtures', fixture)], {
    encoding: 'utf8'
  });
}

test('accepts staging chaos manifests that meet SLO and security gates', () => {
  const result = runValidator('chaos-manifest.valid.json');

  expect(result.status).toBe(0);
  expect(result.stdout).toContain('is valid for staging');
});

test('rejects manifests outside staging safety and SLO bounds', () => {
  const result = runValidator('chaos-manifest.invalid.json');

  expect(result.status).toBe(1);
  expect(result.stderr).toContain('environment must be staging');
  expect(result.stderr).toContain('maxP99LatencyMs must be <= 100');
  expect(result.stderr).toContain('minAvailabilityPercent must be >= 99.99');
  expect(result.stderr).toContain('requireSecurityReview must be true');
});
