#!/usr/bin/env bash
# Installs the competitor arms at the held-out bench's exact versions into $BENCH_TOOLS (default: the main
# checkout's .bench-tools/, untracked). Same sources, flags and checks as the fork's bench/heldout/arms/ARMS.md
# and the Linux run (03-ho.sh, ab-install.sh); only the install folder differs. Safe to rerun.
#   node   v26.13.0 from nodejs.org, sha256 = SHASUMS256.txt (every npm step uses it; nothing from PATH)
#   uv     0.13.7 release asset, sha256 = its .sha256 file; it installs CPython 3.12.16 for bh's venv
#   ad-14  agent-desktop 1.4.0  gh release v1.4.0 (gshost1/agent-desktop), checked against checksums.txt
#   pw     @playwright/cli 0.1.24  npm ci from tools/locks/ (whole tree pinned, registry integrity checked)
#   ab     agent-browser 0.42.0    npm ci from tools/locks/; packaged linux-x64 binary = the release digest
#   bh     browser-harness 0.1.13  binary wheels only (pip download --only-binary=:all:), every wheel = PyPI sha256
#   claude Claude Code 2.1.322 (the Linux pass's version) npm ci from tools/locks/, for model passes
# All npm steps: --ignore-scripts --no-audit --no-fund.
# Usage: bench/tools/install.sh [--python <python3.12>]   (default: uv's pinned CPython 3.12.16)
set -euo pipefail
NODE_V=v26.13.0 UV_V=0.13.7 PY_V=3.12.16
PY=""
[[ ${1:-} == --python ]] && PY=$2
here=$(cd "$(dirname "$0")" && pwd)
common=$(git -C "$here" rev-parse --path-format=absolute --git-common-dir 2>/dev/null || true)
T=${BENCH_TOOLS:-${common:+$(dirname "$common")/.bench-tools}}
[[ -n $T ]] || { echo "set BENCH_TOOLS" >&2; exit 2; }
mkdir -p "$T/downloads" "$T/skills"
T=$(cd "$T" && pwd)
echo "tools: $T"
arch=$(uname -m)
case $arch in x86_64) na=x64 ;; aarch64) na=arm64 ;; *) echo "unsupported arch $arch" >&2; exit 2 ;; esac

# Node, exact version
N=$T/node-$NODE_V
if [[ ! -x $N/bin/node ]]; then
  d=$T/downloads/node-$NODE_V; rm -rf "$d"; mkdir -p "$d"
  f=node-$NODE_V-linux-$na.tar.xz
  curl -fsSL -o "$d/SHASUMS256.txt" "https://nodejs.org/dist/$NODE_V/SHASUMS256.txt"
  curl -fsSL -o "$d/$f" "https://nodejs.org/dist/$NODE_V/$f"
  (cd "$d" && grep " $f\$" SHASUMS256.txt | sha256sum -c -)
  mkdir -p "$N"; tar -xJf "$d/$f" -C "$N" --strip-components=1
fi
export PATH=$N/bin:/usr/bin:/bin
[[ $(node --version) == "$NODE_V" ]] || { echo "node is not $NODE_V" >&2; exit 1; }

npm_ci() {  # <folder name>: the committed package.json + package-lock.json, installed exactly
  local p=$T/$1
  mkdir -p "$p"
  cp "$here/locks/$1/package.json" "$here/locks/$1/package-lock.json" "$p/"
  if ! cmp -s "$here/locks/$1/package-lock.json" "$p/node_modules/.package-lock.json.bench" 2>/dev/null; then
    PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 npm ci --prefix "$p" --ignore-scripts --no-audit --no-fund
    cp "$here/locks/$1/package-lock.json" "$p/node_modules/.package-lock.json.bench"
  fi
}

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
npm_ci playwright-cli-0.1.24
mkdir -p "$T/skills/playwright-cli"
cp -R "$T/playwright-cli-0.1.24/node_modules/@playwright/cli/skills/playwright-cli/." "$T/skills/playwright-cli/"

# agent-browser 0.42.0
npm_ci agent-browser-0.42.0
AB=$T/agent-browser-0.42.0/node_modules/agent-browser/bin/agent-browser-linux-$na
chmod +x "$AB"
if [[ $arch == x86_64 ]]; then
  echo "b69e0f5ea76dcb51f8f75da6bcb87c7f4d81f79b5738f1f08bd121b51cb3dc57  $AB" | sha256sum -c -
fi

# Claude Code 2.1.322 (model passes only); with --ignore-scripts the runner calls the platform binary directly
npm_ci claude-code-2.1.322
CC=$T/claude-code-2.1.322/node_modules/@anthropic-ai/claude-code-linux-$na/claude

# Python for bh: uv's pinned CPython unless --python was given
if [[ -z $PY ]]; then
  UV=$T/uv-$UV_V/uv
  if [[ ! -x $UV ]]; then
    d=$T/downloads/uv-$UV_V; rm -rf "$d"; mkdir -p "$d" "$T/uv-$UV_V"
    f=uv-$arch-unknown-linux-gnu.tar.gz
    curl -fsSL -o "$d/$f" "https://github.com/astral-sh/uv/releases/download/$UV_V/$f"
    curl -fsSL -o "$d/$f.sha256" "https://github.com/astral-sh/uv/releases/download/$UV_V/$f.sha256"
    (cd "$d" && sha256sum -c "$f.sha256")
    tar -xzf "$d/$f" -C "$T/uv-$UV_V" --strip-components=1
  fi
  export UV_PYTHON_INSTALL_DIR=$T/python UV_NO_CONFIG=1
  "$UV" python install "$PY_V" >/dev/null
  PY=$("$UV" python find "$PY_V")
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

{
  echo "installed $(date -u +%FT%TZ) on $(uname -sm)"
  echo "node: $(node --version)"
  echo "claude: $(DISABLE_AUTOUPDATER=1 "$CC" --version 2>&1 | head -1)"
  echo "agent-desktop: $("$T/agent-desktop-1.4.0/agent-desktop" --version 2>&1 | head -1)"
  echo "playwright-cli: $(cd "$T/playwright-cli-0.1.24" && npm ls --depth=0 2>/dev/null | grep @playwright/cli)"
  echo "agent-browser: $("$AB" --version 2>&1 | head -1)"
  echo "browser-harness: $("$T/bh-venv/bin/python" -m pip show browser-harness 2>/dev/null | grep ^Version)"
  echo "python (bh): $("$T/bh-venv/bin/python" --version)"
  echo "chrome: $( (google-chrome --version || chromium --version) 2>/dev/null)"
} | tee "$T/VERSIONS.txt"
