// One entry per target the release workflow builds. Keep in step with the
// build matrix in .github/workflows/release.yml.
const PLATFORMS = {
  'darwin-arm64': {
    target: 'aarch64-apple-darwin',
    binaryName: 'tabref-darwin-arm64',
    archive: 'tar.gz',
    member: 'tabref',
  },
  'linux-x64': {
    target: 'x86_64-unknown-linux-gnu',
    binaryName: 'tabref-linux-x64',
    archive: 'tar.gz',
    member: 'tabref',
  },
  'win32-x64': {
    target: 'x86_64-pc-windows-msvc',
    binaryName: 'tabref-win32-x64.exe',
    archive: 'zip',
    member: 'tabref.exe',
  },
};

function resolve(osPlatform, osArch) {
  return PLATFORMS[`${osPlatform}-${osArch}`];
}

function platformKeys() {
  return Object.keys(PLATFORMS);
}

// The release names archives after the tag, e.g. tabref-v0.2.0-<target>.tar.gz,
// with every file inside one top directory of the same name.
function archiveStem(version, target) {
  return `tabref-v${version}-${target}`;
}

function archiveName(version, entry) {
  return `${archiveStem(version, entry.target)}.${entry.archive}`;
}

module.exports = { PLATFORMS, archiveName, archiveStem, platformKeys, resolve };
