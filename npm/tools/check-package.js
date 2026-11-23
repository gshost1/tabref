#!/usr/bin/env node

// Release-time only, not shipped. Fails unless the packed tarball holds the
// launcher, postinstall, and a binary plus digest for every platform, and
// unless the package version matches the expected release version.
//
//   node npm/tools/check-package.js EXPECTED_VERSION
const { execFileSync } = require('child_process');
const { join } = require('path');
const { PLATFORMS } = require('../lib/platform');

const expectedVersion = process.argv[2];
if (!expectedVersion) {
  console.error('Usage: node npm/tools/check-package.js EXPECTED_VERSION');
  process.exit(2);
}

const npmDir = join(__dirname, '..');
const [report] = JSON.parse(execFileSync('npm', ['pack', '--dry-run', '--json'], {
  cwd: npmDir,
  encoding: 'utf8',
}));

const problems = [];
if (report.name !== 'tabref') problems.push(`package name is ${report.name}, expected tabref`);
if (report.version !== expectedVersion) {
  problems.push(`package version is ${report.version}, expected ${expectedVersion}`);
}

const files = new Map(report.files.map((file) => [file.path, file]));
const required = ['package.json', 'bin/tabref', 'lib/platform.js', 'lib/windows-bin.js', 'scripts/postinstall.js', 'LICENSE', 'NOTICE'];
for (const entry of Object.values(PLATFORMS)) {
  required.push(`bin/${entry.binaryName}`, `bin/${entry.binaryName}.sha256`);
}
for (const path of required) {
  if (!files.has(path)) problems.push(`missing ${path}`);
  else if (files.get(path).size === 0) problems.push(`empty ${path}`);
}
for (const path of files.keys()) {
  if (path.startsWith('tools/')) problems.push(`release tooling leaked into the package: ${path}`);
}

if (problems.length) {
  for (const problem of problems) console.error(`FAIL: ${problem}`);
  process.exit(1);
}
console.log(`OK: ${report.filename} holds ${files.size} files, ${Object.keys(PLATFORMS).length} platform binaries, version ${report.version}`);
