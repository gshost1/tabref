#!/usr/bin/env node

// The package ships every prebuilt binary. postinstall checks the one for
// this machine against its recorded digest and links the `tabref` command
// straight to it, so each call skips the Node launcher.
const { createHash } = require('crypto');
const {
  chmodSync,
  existsSync,
  linkSync,
  lstatSync,
  readFileSync,
  renameSync,
  rmSync,
  unlinkSync,
} = require('fs');
const { basename, dirname, join } = require('path');
const { arch, platform } = require('os');
const { platformKeys, resolve } = require('../lib/platform');

const binDir = join(__dirname, '..', 'bin');
const BIN_ENTRY_NAME = 'tabref';

function log(message) {
  process.stderr.write(`tabref: ${message}\n`);
}

function sha256(path) {
  return createHash('sha256').update(readFileSync(path)).digest('hex');
}

function matchesRecordedDigest(path) {
  try {
    return readFileSync(`${path}.sha256`, 'utf8').trim() === sha256(path);
  } catch {
    return false;
  }
}

// Only link inside a real install (…/node_modules/@gshost1/tabref/bin),
// never in the source checkout.
function isInstalledPackage(directory) {
  const scope = dirname(dirname(directory));
  return basename(scope) === '@gshost1' && basename(dirname(scope)) === 'node_modules';
}

function isNodeLauncher(path) {
  try {
    return readFileSync(path).subarray(0, 2).toString() === '#!';
  } catch {
    return false;
  }
}

// pnpm's bin entries are shell shims that run `node bin/tabref`, on every
// OS, so swapping bin/tabref for the native binary would break them. npm,
// yarn and bun symlink (POSIX) or write npm-style shims (Windows).
function launcherRequired(userAgent = process.env.npm_config_user_agent || '') {
  return userAgent.startsWith('pnpm/');
}

function linkBinEntry(directory, binaryName, osPlatform = platform()) {
  if (!isInstalledPackage(directory) || launcherRequired()) return false;
  if (osPlatform === 'win32') {
    return require('../lib/windows-bin')(directory, binaryName);
  }
  const entryPath = join(directory, BIN_ENTRY_NAME);
  const binaryPath = join(directory, binaryName);
  const temporary = `${entryPath}.link-${process.pid}`;
  try {
    const binaryStat = lstatSync(binaryPath);
    try {
      const entryStat = lstatSync(entryPath);
      if (entryStat.ino === binaryStat.ino && entryStat.dev === binaryStat.dev) return false;
    } catch {}
    rmSync(temporary, { force: true });
    linkSync(binaryPath, temporary);
    renameSync(temporary, entryPath);
    return true;
  } catch (err) {
    if (isNodeLauncher(entryPath)) {
      log(`could not link the command to the native binary, keeping the Node launcher: ${err.message}`);
    } else {
      log(`could not link the command to the native binary: ${err.message}`);
      process.exitCode = 1;
    }
    return false;
  } finally {
    try { unlinkSync(temporary); } catch {}
  }
}

function main() {
  const entry = resolve(platform(), arch());
  if (!entry) {
    log(`no prebuilt binary for ${platform()}-${arch()}; prebuilt platforms: ${platformKeys().join(', ')}.`);
    log('Build from source: https://github.com/gshost1/tabref');
    process.exitCode = 1;
    return;
  }

  // In the source checkout (npm install or npm link inside npm/) there are
  // no bundled binaries yet; nothing to check or link.
  if (!isInstalledPackage(binDir)) return;

  const binaryPath = join(binDir, entry.binaryName);
  if (!existsSync(binaryPath) || !matchesRecordedDigest(binaryPath)) {
    log(`the bundled binary ${entry.binaryName} is missing or does not match its recorded digest.`);
    log('Reinstall @gshost1/tabref, or download a release from https://github.com/gshost1/tabref/releases');
    process.exitCode = 1;
    return;
  }

  if (platform() !== 'win32') chmodSync(binaryPath, 0o755);
  if (linkBinEntry(binDir, entry.binaryName)) {
    log('native binary linked: each call runs without the Node launcher.');
  }
}

if (require.main === module) {
  main();
}

module.exports = { launcherRequired, linkBinEntry, matchesRecordedDigest };
