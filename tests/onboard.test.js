'use strict';

const { parseArgs, checkTool, buildReport, remediationFor } = require('../scripts/onboard');

describe('local onboarding script', () => {
  test('parses supported flags', () => {
    expect(parseArgs(['--install', '--verify', '--json'])).toEqual({
      install: true,
      verify: true,
      json: true,
      help: false
    });
  });

  test('rejects unknown flags', () => {
    expect(() => parseArgs(['--unsafe'])).toThrow('Unknown option: --unsafe');
  });

  test('reports successful tool checks with version output', () => {
    const runner = jest.fn(() => ({ status: 0, stdout: 'v20.0.0\n', stderr: '' }));
    expect(checkTool({ name: 'Node.js', command: 'node', args: ['--version'], required: true }, runner)).toMatchObject({
      ok: true,
      version: 'v20.0.0',
      remediation: null
    });
  });

  test('fails report only when required checks fail', () => {
    const checks = [
      { name: 'Required', command: 'required', args: ['--version'], required: true },
      { name: 'Optional', command: 'optional', args: ['--version'], required: false }
    ];
    const runner = jest.fn((command) => ({ status: command === 'optional' ? 0 : 127, stdout: '', stderr: '' }));
    const report = buildReport({}, checks, runner);
    expect(report.ok).toBe(false);
    expect(report.results).toHaveLength(2);
    expect(report.results[1].ok).toBe(true);
  });

  test('provides tool-specific remediation', () => {
    expect(remediationFor('cargo')).toContain('rustup.rs');
  });
});
