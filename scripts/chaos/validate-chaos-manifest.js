#!/usr/bin/env node
const fs = require('fs');

const manifestPath = process.argv[2];
if (!manifestPath) {
  console.error('Usage: validate-chaos-manifest.js <manifest.json>');
  process.exit(2);
}

const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
const errors = [];

function requirePath(path) {
  const value = path.split('.').reduce((current, key) => current && current[key], manifest);
  if (value === undefined || value === null || value === '') errors.push(`missing ${path}`);
  return value;
}

requirePath('metadata.name');
const environment = requirePath('metadata.environment');
requirePath('metadata.owner');
requirePath('spec.target.service');
requirePath('spec.fault.type');
requirePath('spec.fault.duration');
const p99 = requirePath('spec.safeguards.maxP99LatencyMs');
const availability = requirePath('spec.safeguards.minAvailabilityPercent');
const securityReview = requirePath('spec.safeguards.requireSecurityReview');
requirePath('spec.rollback.strategy');

if (environment !== 'staging') errors.push('environment must be staging');
if (typeof p99 === 'number' && p99 > 100) errors.push('maxP99LatencyMs must be <= 100');
if (typeof availability === 'number' && availability < 99.99) errors.push('minAvailabilityPercent must be >= 99.99');
if (securityReview !== true) errors.push('requireSecurityReview must be true');

if (errors.length) {
  console.error(`Invalid chaos manifest: ${errors.join(', ')}`);
  process.exit(1);
}

console.log(`Chaos manifest ${manifest.metadata.name} is valid for staging.`);
