#!/usr/bin/env bash
# Installs the competitor arms at the held-out bench's exact versions into $BENCH_TOOLS (default: the main
# checkout's .bench-tools/, untracked). Same sources, flags and checks as the fork's bench/heldout/arms/ARMS.md
# and the Linux run (03-ho.sh, ab-install.sh); only the install folder differs. Safe to rerun.
#   ad-14  agent-desktop 1.4.0  gh release v1.4.0 (gshost1/agent-desktop), checked against checksums.txt
#   pw     @playwright/cli 0.1.24  npm --ignore-scripts (no browser download)
#   ab     agent-browser 0.42.0    npm --ignore-scripts; packaged linux-x64 binary checked against the release digest
#   bh     browser-harness 0.1.13  binary wheels only (pip download --only-binary=:all:), wheel sha256 = PyPI
# Usage: bench/tools/install.sh [--python python3.12]
set -euo pipefail
PY=python3.12
[[ ${1:-} == --python ]] && PY=$2
here=$(cd "$(dirname "$0")" && pwd)
common=$(git -C "$here" rev-parse --path-format=absolute --git-common-dir 2>/dev/null || true)
T=${BENCH_TOOLS:-${common:+$(dirname "$common")/.bench-tools}}
[[ -n $T ]] || { echo "set BENCH_TOOLS" >&2; exit 2; }
mkdir -p "$T/downloads" "$T/skills"
T=$(cd "$T" && pwd)
echo "tools: $T"
arch=$(uname -m)
[[ $arch == x86_64 || $arch == aarch64 ]] || { echo "unsupported arch $arch" >&2; exit 2; }

# agent-desktop 1.4.0 (fork release)
AD=agent-desktop-v1.4.0-$arch-unknown-linux-gnu.tar.gz
if [[ ! -x $T/agent-desktop-1.4.0/agent-desktop ]]; then
  d=$T/downloads/agent-desktop-1.4.0; rm -rf "$d"; mkdir -p "$d" "$T/agent-desktop-1.4.0"
  if command -v gh >/dev/null && gh auth status >/dev/null 2>&1; then
    (cd "$d" && gh release download v1.4.0 --repo gshost1/agent-desktop -p "$AD" -p checksums.txt)
  else
    for f in "$AD" checksums.txt; do  # the fork is public; the same release assets over plain HTTPS
      curl -fsSL -o "$d/$f" "https://github.com/gshost1/agent-desktop/releases/download/v1.4.0/$f"
    done
  fi
fi
(cd "$T/downloads/agent-desktop-1.4.0" && grep " $AD\$" checksums.txt | sha256sum -c -)
[[ -x $T/agent-desktop-1.4.0/agent-desktop ]] || tar -xzf "$T/downloads/agent-desktop-1.4.0/$AD" -C "$T/agent-desktop-1.4.0"
[[ -x $T/agent-desktop-1.4.0/agent-desktop ]] || { echo "agent-desktop binary not at the archive root" >&2; exit 1; }

# @playwright/cli 0.1.24
if [[ ! -x $T/playwright-cli-0.1.24/node_modules/.bin/playwright-cli ]]; then
  mkdir -p "$T/playwright-cli-0.1.24"
  PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 npm install --prefix "$T/playwright-cli-0.1.24" --ignore-scripts --no-audit --no-fund \
    --save-exact @playwright/cli@0.1.24
fi
mkdir -p "$T/skills/playwright-cli"
cp -R "$T/playwright-cli-0.1.24/node_modules/@playwright/cli/skills/playwright-cli/." "$T/skills/playwright-cli/"

# agent-browser 0.42.0
if [[ ! -d $T/agent-browser-0.42.0/node_modules/agent-browser ]]; then
  mkdir -p "$T/agent-browser-0.42.0"
  npm install --prefix "$T/agent-browser-0.42.0" --ignore-scripts --no-audit --no-fund --save-exact agent-browser@0.42.0
fi
AB=$T/agent-browser-0.42.0/node_modules/agent-browser/bin/agent-browser-linux-${arch/x86_64/x64}
AB=${AB/aarch64/arm64}
chmod +x "$AB"
if [[ $arch == x86_64 ]]; then
  echo "b69e0f5ea76dcb51f8f75da6bcb87c7f4d81f79b5738f1f08bd121b51cb3dc57  $AB" | sha256sum -c -
fi

# browser-harness 0.1.13 from binary wheels
W=$T/downloads/browser-harness-0.1.13
if [[ ! -x $T/bh-venv/bin/browser-harness ]]; then
  rm -rf "$W" "$T/bh-venv"; mkdir -p "$W"
  "$PY" -m venv "$T/bh-venv"
  "$T/bh-venv/bin/python" -m pip download --disable-pip-version-check --only-binary=:all: -d "$W" browser-harness==0.1.13
fi
whl=$W/browser_harness-0.1.13-py3-none-any.whl
echo "2491459e4bfc0ee8aea22dc6c4680fc0f791b7ba553446323c50d2883449d769  $whl" | sha256sum -c -
"$T/bh-venv/bin/python" -I - "$W" <<'PY'
import hashlib, json, pathlib, sys, urllib.request
bad = []
for whl in sorted(pathlib.Path(sys.argv[1]).glob("*.whl")):
    name, version = whl.name.split("-")[:2]
    meta = json.load(urllib.request.urlopen(f"https://pypi.org/pypi/{name}/{version}/json", timeout=20))
    want = {u["filename"]: u["digests"]["sha256"] for u in meta["urls"]}
    got = hashlib.sha256(whl.read_bytes()).hexdigest()
    print(f"{whl.name}: {'OK' if want.get(whl.name) == got else 'MISMATCH'}")
    if want.get(whl.name) != got:
        bad.append(whl.name)
sys.exit(1 if bad else 0)
PY
[[ -x $T/bh-venv/bin/browser-harness ]] || \
  "$T/bh-venv/bin/python" -m pip install --disable-pip-version-check --no-index --find-links "$W" --only-binary=:all: browser-harness==0.1.13

# Claude Code 2.1.322, the version the Linux pass ran (model runs only). npm checks the registry integrity; with
# --ignore-scripts the wrapper's postinstall is skipped, so the runner calls the platform binary directly.
CC=$T/claude-code-2.1.322/node_modules/@anthropic-ai/claude-code-linux-${arch/x86_64/x64}/claude
CC=${CC/aarch64/arm64}
if [[ ! -x $CC ]]; then
  mkdir -p "$T/claude-code-2.1.322"
  npm install --prefix "$T/claude-code-2.1.322" --ignore-scripts --no-audit --no-fund --save-exact @anthropic-ai/claude-code@2.1.322
fi

{
  echo "installed $(date -u +%FT%TZ) on $(uname -sm)"
  echo "claude: $(DISABLE_AUTOUPDATER=1 "$CC" --version 2>&1 | head -1)"
  echo "agent-desktop: $("$T/agent-desktop-1.4.0/agent-desktop" --version 2>&1 | head -1)"
  echo "playwright-cli: $(cd "$T/playwright-cli-0.1.24" && npm ls --depth=0 2>/dev/null | grep @playwright/cli)"
  echo "agent-browser: $("$AB" --version 2>&1 | head -1)"
  echo "browser-harness: $("$T/bh-venv/bin/python" -m pip show browser-harness 2>/dev/null | grep ^Version)"
  echo "python (bh): $("$T/bh-venv/bin/python" --version)"
  echo "node: $(node --version)"
  echo "chrome: $( (google-chrome --version || chromium --version) 2>/dev/null)"
} | tee "$T/VERSIONS.txt"
