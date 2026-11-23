#!/usr/bin/env node

// Release-time only, not shipped. Takes the directory of release archives
// plus SHA256SUMS, checks each archive's digest, and writes every platform's
// binary into npm/bin with a .sha256 sidecar that postinstall checks.
//
//   node npm/tools/prepare-binaries.js ARCHIVE_DIR [platform-key ...]
//
// With no keys it prepares every platform. Keys exist for local testing.
const { createHash } = require('crypto');
const { execFileSync } = require('child_process');
const { mkdirSync, readFileSync, writeFileSync } = require('fs');
const { join } = require('path');
const { PLATFORMS, archiveName, archiveStem } = require('../lib/platform');

const [archiveDir, ...keys] = process.argv.slice(2);
if (!archiveDir) {
  console.error('Usage: node npm/tools/prepare-binaries.js ARCHIVE_DIR [platform-key ...]');
  process.exit(2);
}

const npmDir = join(__dirname, '..');
const binDir = join(npmDir, 'bin');
const { version } = JSON.parse(readFileSync(join(npmDir, 'package.json'), 'utf8'));
const checksums = readFileSync(join(archiveDir, 'SHA256SUMS'), 'utf8');

function sha256(buffer) {
  return createHash('sha256').update(buffer).digest('hex');
}

function checksumFor(fileName) {
  for (const line of checksums.split('\n')) {
    const match = line.trim().match(/^([0-9a-fA-F]{64})\s+\*?(.+)$/);
    if (match && match[2] === fileName) return match[1].toLowerCase();
  }
  throw new Error(`SHA256SUMS has no entry for ${fileName}`);
}

function extract(archivePath, entry, memberPath) {
  const options = { maxBuffer: 256 * 1024 * 1024 };
  return entry.archive === 'zip'
    ? execFileSync('unzip', ['-p', archivePath, memberPath], options)
    : execFileSync('tar', ['-xOzf', archivePath, memberPath], options);
}

const selected = keys.length ? keys : Object.keys(PLATFORMS);
mkdirSync(binDir, { recursive: true });
for (const key of selected) {
  const entry = PLATFORMS[key];
  if (!entry) throw new Error(`Unknown platform key: ${key}`);
  const name = archiveName(version, entry);
  const archivePath = join(archiveDir, name);
  if (sha256(readFileSync(archivePath)) !== checksumFor(name)) {
    throw new Error(`Checksum mismatch for ${name}`);
  }
  const payload = extract(archivePath, entry, `${archiveStem(version, entry.target)}/${entry.member}`);
  if (payload.length === 0) throw new Error(`Empty binary in ${name}`);
  writeFileSync(join(binDir, entry.binaryName), payload, { mode: 0o755 });
  writeFileSync(join(binDir, `${entry.binaryName}.sha256`), `${sha256(payload)}\n`);
  console.log(`${entry.binaryName}  ${sha256(payload)}  (from ${name})`);
}
