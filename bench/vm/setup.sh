#!/usr/bin/env bash
# Root, on a fresh boxd Ubuntu 24.04 VM: what the bench needs to run any mode there, the way the Linux pass ran
# (user `bench` without sudo, Chrome from Google's apt repo or Chromium, Node, Python 3.12, Xvfb + openbox + AT-SPI
# for the private display session), then the arms via bench/tools/install.sh into /home/bench/bench-tools.
# Expects the bench folder at /tmp/tabref-bench (vm/boxd.sh copies it). Usage: setup.sh [--chromium]
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
apt-get update -q
apt-get install -y -q --no-install-recommends ca-certificates curl gnupg git jq python3 python3-venv python3.12-venv \
  xvfb openbox at-spi2-core dbus dbus-user-session fonts-dejavu fonts-liberation fonts-noto-core locales xz-utils
locale-gen en_US.UTF-8 >/dev/null
if [[ ${1:-} == --chromium ]]; then
  apt-get install -y -q chromium-browser || apt-get install -y -q chromium
  ln -sfn "$(command -v chromium-browser || command -v chromium)" /usr/local/bin/bench-chrome
elif ! command -v google-chrome >/dev/null; then
  curl -fsSL https://dl.google.com/linux/linux_signing_key.pub | gpg --dearmor -o /usr/share/keyrings/google-chrome.gpg
  echo 'deb [arch=amd64 signed-by=/usr/share/keyrings/google-chrome.gpg] https://dl.google.com/linux/chrome/deb/ stable main' \
    > /etc/apt/sources.list.d/google-chrome.list
  apt-get update -q && apt-get install -y -q google-chrome-stable
fi
if [[ ! -x /opt/node/bin/node ]]; then
  W=$(mktemp -d); cd "$W"
  curl -fsSL https://nodejs.org/dist/latest-v24.x/SHASUMS256.txt -o S.txt
  A=$(awk '$2 ~ /^node-v24\.[0-9]+\.[0-9]+-linux-x64.tar.xz$/ {print $2}' S.txt)
  curl -fsSL "https://nodejs.org/dist/latest-v24.x/$A" -o "$A"; grep " $A\$" S.txt | sha256sum -c -
  install -d /opt/node; tar -xf "$A" -C /opt/node --strip-components=1
  cd /; rm -rf "$W"
fi
for b in node npm npx; do ln -sfn /opt/node/bin/$b /usr/local/bin/$b; done
id bench >/dev/null 2>&1 || useradd --create-home --shell /bin/bash bench
chmod 700 /home/bench
rm -rf /home/bench/tabref-bench && cp -R /tmp/tabref-bench /home/bench/tabref-bench && chown -R bench:bench /home/bench/tabref-bench
sudo -u bench -H env BENCH_TOOLS=/home/bench/bench-tools PATH=/usr/local/bin:/usr/bin:/bin \
  bash /home/bench/tabref-bench/bench/tools/install.sh --python python3.12
echo setup-done
