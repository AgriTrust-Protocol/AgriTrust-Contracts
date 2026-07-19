const fs = require('fs');
const path = require('path');

const { checkFile } = require('../scripts/pre-commit-quality');

const repoRoot = path.resolve(__dirname, '..');
const tempDir = path.join(repoRoot, '.tmp-precommit-tests');

function writeTempFile(name, contents) {
  fs.mkdirSync(tempDir, { recursive: true });
  const filePath = path.join(tempDir, name);
  fs.writeFileSync(filePath, contents);
  return path.relative(repoRoot, filePath);
}

afterAll(() => {
  fs.rmSync(tempDir, { recursive: true, force: true });
});

test('accepts clean text files', () => {
  const file = writeTempFile('clean.txt', 'alpha\nbeta\n');
  expect(checkFile(file)).toEqual([]);
});

test('reports whitespace and newline violations', () => {
  const file = writeTempFile('dirty.txt', 'alpha  \r\nbeta');
  expect(checkFile(file)).toEqual(expect.arrayContaining([
    'contains CRLF line endings',
    'missing trailing newline',
    'trailing whitespace on line 1',
  ]));
});

test('reports high-confidence secrets', () => {
  const file = writeTempFile('secret.txt', `token=${'AKIA' + '1234567890ABCDEF'}\n`);
  expect(checkFile(file)).toContain('possible secret detected: AWS access key');
});

test('skips binary files', () => {
  fs.mkdirSync(tempDir, { recursive: true });
  const absolutePath = path.join(tempDir, 'binary.bin');
  fs.writeFileSync(absolutePath, Buffer.from([0, 1, 2, 3]));
  expect(checkFile(path.relative(repoRoot, absolutePath))).toEqual([]);
});
