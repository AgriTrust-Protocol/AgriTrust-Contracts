#!/usr/bin/env node
'use strict';

const { spawnSync } = require('child_process');
const fs = require('fs');
const path = require('path');

const DEFAULT_CHECKS = [
  { name: 'Node.js', command: 'node', args: ['--version'], required: true },
  { name: 'npm', command: 'npm', args: ['--version'], required: true },
  { name: 'Rust cargo', command: 'cargo', args: ['--version'], required: false },
  { name: 'Stellar CLI', command: 'stellar', args: ['--version'], required: false },
  { name: 'Foundry forge', command: 'forge', args: ['--version'], required: false }
];

function parseArgs(argv) {
  const options = { install: false, verify: false, json: false, help: false };
  for (const arg of argv) {
    if (arg === '--install') options.install = true;
    else if (arg === '--verify') options.verify = true;
    else if (arg === '--json') options.json = true;
    else if (arg === '--help' || arg === '-h') options.help = true;
    else throw new Error(`Unknown option: ${arg}`);
  }
  return options;
}

function run(command, args, opts = {}) {
  return spawnSync(command, args, { encoding: 'utf8', stdio: opts.stdio || 'pipe', cwd: opts.cwd || process.cwd() });
}

function checkTool(check, runner = run) {
  const result = runner(check.command, check.args);
  return {
    name: check.name,
    command: `${check.command} ${check.args.join(' ')}`,
    required: check.required,
    ok: result.status === 0,
    version: result.status === 0 ? (result.stdout || result.stderr).trim().split('\n')[0] : null,
    remediation: result.status === 0 ? null : remediationFor(check.command)
  };
}

function remediationFor(command) {
  return {
    node: 'Install Node.js 20+ from https://nodejs.org/.',
    npm: 'Install npm with Node.js or your OS package manager.',
    cargo: 'Install Rust from https://rustup.rs/.',
    stellar: 'Install Stellar CLI from https://developers.stellar.org/docs/tools/stellar-cli.',
    forge: 'Install Foundry from https://book.getfoundry.sh/getting-started/installation.'
  }[command] || `Install ${command} and ensure it is on PATH.`;
}

function installDependencies(runner = run, cwd = process.cwd()) {
  const lockfile = path.join(cwd, 'package-lock.json');
  const command = fs.existsSync(lockfile) ? 'ci' : 'install';
  return runner('npm', [command], { stdio: 'inherit', cwd });
}

function verifyProject(runner = run, cwd = process.cwd()) {
  return runner('npm', ['test', '--', '--runInBand', '--forceExit'], { stdio: 'inherit', cwd });
}

function buildReport(options, checks = DEFAULT_CHECKS, runner = run) {
  const startedAt = Date.now();
  const results = checks.map((check) => checkTool(check, runner));
  const failedRequired = results.filter((result) => result.required && !result.ok);
  return { ok: failedRequired.length === 0, durationMs: Date.now() - startedAt, results };
}

function printReport(report) {
  console.log('AgriTrust local development setup check');
  for (const result of report.results) {
    const marker = result.ok ? '✓' : result.required ? '✗' : '!';
    console.log(`${marker} ${result.name}${result.version ? ` (${result.version})` : ''}`);
    if (!result.ok) console.log(`  ${result.required ? 'Required' : 'Optional'}: ${result.remediation}`);
  }
}

function printHelp() {
  console.log(`Usage: npm run setup:local -- [--install] [--verify] [--json]\n\nChecks required and optional local development tools.\n\nOptions:\n  --install  Run npm ci/install after tool checks pass\n  --verify   Run the test suite after setup\n  --json     Emit machine-readable setup report\n  --help     Show this help message`);
}

function main(argv = process.argv.slice(2), runner = run, cwd = process.cwd()) {
  const options = parseArgs(argv);
  if (options.help) { printHelp(); return 0; }

  const report = buildReport(options, DEFAULT_CHECKS, runner);
  if (options.json) console.log(JSON.stringify(report, null, 2));
  else printReport(report);

  if (!report.ok) return 1;
  if (options.install && installDependencies(runner, cwd).status !== 0) return 1;
  if (options.verify && verifyProject(runner, cwd).status !== 0) return 1;
  return 0;
}

if (require.main === module) {
  try { process.exitCode = main(); }
  catch (error) { console.error(error.message); process.exitCode = 1; }
}

module.exports = { DEFAULT_CHECKS, parseArgs, checkTool, buildReport, main, remediationFor };
