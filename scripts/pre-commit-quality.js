#!/usr/bin/env node
'use strict';

const { execFileSync } = require('child_process');
const fs = require('fs');
const path = require('path');

const REPO_ROOT = path.resolve(__dirname, '..');
const MAX_BYTES = 1024 * 1024;
const BLOCKED_PATTERNS = [
  { name: 'AWS access key', regex: /AKIA[0-9A-Z]{16}/ },
  { name: 'private key block', regex: /-----BEGIN (?:RSA |EC |OPENSSH |DSA )?PRIVATE KEY-----/ },
  { name: 'GitHub token', regex: /gh[pousr]_[A-Za-z0-9_]{36,255}/ },
  { name: 'Slack token', regex: /xox[baprs]-[A-Za-z0-9-]{10,}/ },
];

function git(args, options = {}) {
  const result = execFileSync('git', args, { cwd: REPO_ROOT, ...options });
  return Buffer.isBuffer(result) ? result : result.trim();
}

function stagedFiles() {
  const output = git(['diff', '--cached', '--name-only', '--diff-filter=ACMR'], { encoding: 'utf8' });
  return output ? output.split('\n').filter(Boolean) : [];
}

function isBinary(buffer) {
  return buffer.includes(0);
}

function readStagedFile(relativePath) {
  const objectPath = `:${relativePath}`;
  const size = Number(git(['cat-file', '-s', objectPath], { encoding: 'utf8' }));
  if (size > MAX_BYTES) return null;
  return git(['show', objectPath]);
}

function checkFile(relativePath, buffer = null) {
  if (!buffer) {
    const fullPath = path.join(REPO_ROOT, relativePath);
    const stat = fs.statSync(fullPath);
    if (stat.size > MAX_BYTES) return [];
    buffer = fs.readFileSync(fullPath);
  }
  if (isBinary(buffer)) return [];

  const text = buffer.toString('utf8');
  const errors = [];

  if (!text.endsWith('\n')) errors.push('missing trailing newline');
  if (/\r\n?/.test(text)) errors.push('contains CRLF line endings');

  text.split('\n').forEach((line, index) => {
    if (/[ \t]+\r?$/.test(line)) errors.push(`trailing whitespace on line ${index + 1}`);
  });

  for (const pattern of BLOCKED_PATTERNS) {
    if (pattern.regex.test(text)) errors.push(`possible secret detected: ${pattern.name}`);
  }

  return errors;
}

function main() {
  const start = process.hrtime.bigint();
  const files = stagedFiles();
  const failures = [];

  for (const file of files) {
    const buffer = readStagedFile(file);
    const errors = buffer ? checkFile(file, buffer) : [];
    if (errors.length) failures.push({ file, errors });
  }

  const elapsedMs = Number(process.hrtime.bigint() - start) / 1e6;
  if (failures.length) {
    console.error('Pre-commit quality checks failed:');
    for (const failure of failures) {
      console.error(`\n${failure.file}`);
      for (const error of failure.errors) console.error(`  - ${error}`);
    }
    console.error('\nFix the issues above, stage the fixes, and retry the commit.');
    process.exit(1);
  }

  console.log(`Pre-commit quality checks passed for ${files.length} staged file(s) in ${elapsedMs.toFixed(1)}ms.`);
}

if (require.main === module) main();

module.exports = { checkFile, readStagedFile, stagedFiles };
