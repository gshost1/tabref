#!/usr/bin/env bash
# Laptop side: run one bench mode on a throwaway boxd VM and bring the results back, then destroy the VM.
# Needs a signed-in boxd CLI (or BOXD_TOKEN in the environment); the script never reads token files.
# The VM deletes itself after --hours (default 3) even if this script dies.
#
#   bench/vm/boxd.sh latency              per-command latency, every arm, Google Chrome
#   bench/vm/boxd.sh replay               model-free replay, every arm
#   bench/vm/boxd.sh models <run_web.py args>   a model pass; the key goes to the VM only through
#                                         `boxd machine exec` env (ANTHROPIC_API_KEY must be set here) and is never
#                                         written to the VM's disk
# Options (before the mode): --name NAME --chromium --hours N --keep (do not destroy at the end)
# tabref: pass TABREF_BIN=<local linux x86_64 binary> to copy it into the VM; without it tabref rows are skipped.
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
name=tabref-bench-$(date +%m%d%H%M) chromium="" hours=3 keep=0
while [[ ${1:-} == --* ]]; do
  case $1 in
    --name) name=$2; shift 2 ;;
    --chromium) chromium=--chromium; shift ;;
    --hours) hours=$2; shift 2 ;;
    --keep) keep=1; shift ;;
    *) echo "unknown option $1" >&2; exit 2 ;;
  esac
done
mode=${1:?mode: latency, replay or models}; shift || true
vx() { boxd machine exec "$name" -- bash -c "$1"; }
cleanup() { [[ $keep == 1 ]] || boxd machine remove "$name" --confirm || echo "could not remove $name: remove it by hand" >&2; }
boxd machine new "$name" --delete-after "${hours}h"
trap cleanup EXIT
tmp=$(mktemp -d)
tar -C "$here/.." --exclude='bench/runs' --exclude='__pycache__' -czf "$tmp/bench.tgz" bench
boxd machine cp "$tmp/bench.tgz" "$name:/tmp/bench.tgz"
vx "sudo rm -rf /tmp/tabref-bench && sudo mkdir -p /tmp/tabref-bench && sudo tar -xzf /tmp/bench.tgz -C /tmp/tabref-bench"
vx "sudo bash /tmp/tabref-bench/bench/vm/setup.sh $chromium" | tail -15
chrome=/usr/bin/google-chrome; [[ -n $chromium ]] && chrome=/usr/local/bin/bench-chrome
tabref_env=""
if [[ -n ${TABREF_BIN:-} ]]; then
  boxd machine cp "$TABREF_BIN" "$name:/tmp/tabref"
  vx "sudo install -m755 -o bench /tmp/tabref /home/bench/tabref"
  tabref_env="TABREF_BIN=/home/bench/tabref"
fi
run="sudo -u bench -H env BENCH_TOOLS=/home/bench/bench-tools BENCH_RUNS=/home/bench/runs $tabref_env PATH=/usr/local/bin:/usr/bin:/bin"
B=/home/bench/tabref-bench/bench
case $mode in
  latency) vx "cd $B && $run python3 -I latency/run.py --display headless --chrome $chrome $*" ;;
  replay) vx "cd $B && $run python3 -I replay/run.py --display headless --chrome $chrome $*" ;;
  models)
    : "${ANTHROPIC_API_KEY:?set ANTHROPIC_API_KEY for a model pass}"
    boxd machine exec --env "ANTHROPIC_API_KEY=$ANTHROPIC_API_KEY" "$name" -- bash -c \
      "cd $B && sudo --preserve-env=ANTHROPIC_API_KEY -u bench -H env BENCH_TOOLS=/home/bench/bench-tools BENCH_RUNS=/home/bench/runs $tabref_env PATH=/usr/local/bin:/usr/bin:/bin ANTHROPIC_API_KEY=\"\$ANTHROPIC_API_KEY\" python3 -I harness/run_web.py --display xvfb --chrome $chrome --host boxd $*" ;;
  *) echo "unknown mode $mode" >&2; exit 2 ;;
esac
vx "sudo tar -C /home/bench -czf /tmp/runs.tgz runs"
mkdir -p "$here/runs/boxd-$name"
boxd machine cp "$name:/tmp/runs.tgz" "$here/runs/boxd-$name/runs.tgz"
tar -xzf "$here/runs/boxd-$name/runs.tgz" -C "$here/runs/boxd-$name"
echo "results in $here/runs/boxd-$name/runs (report: BENCH_RUNS=$here/runs/boxd-$name/runs python3 -I bench/report.py all)"
boxd machine usage "$name" 2>/dev/null | tail -5 || true
