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
#   BENCH_FRESH_DIR=<sealed folder> bench/vm/boxd.sh sealed <run_web.py args>
#                                         one judged pass on the sealed fresh set, the way tabbench.sh runs it: the
#                                         original is seal-checked here, copied into the bench user's home as a tar
#                                         stream (the VM copy is deleted after the harness loads it), seal-checked
#                                         there, and checked again here after the pass; the staged rows are committed
#                                         only if the seal held. Only results-web.jsonl comes back with the results;
#                                         the sealed output (answers, verifier text, transcripts) goes to
#                                         bench/runs/fresh-out/<pass id>.tgz (mode 600) and nothing else of runs/.
# Options (before the mode): --name NAME --chromium --hours N --keep (do not destroy at the end)
#   --vcpu N --memory SIZE (default 4 and 16G, the size the Linux passes used)
# tabref: pass TABREF_BIN=<local linux x86_64 binary> to copy it into the VM; without it tabref rows are skipped.
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
name=tabref-bench-$(date +%m%d%H%M) chromium="" hours=3 keep=0 vcpu=4 memory=16G
while [[ ${1:-} == --* ]]; do
  case $1 in
    --name) name=$2; shift 2 ;;
    --chromium) chromium=--chromium; shift ;;
    --hours) hours=$2; shift 2 ;;
    --keep) keep=1; shift ;;
    --vcpu) vcpu=$2; shift 2 ;;
    --memory) memory=$2; shift 2 ;;
    *) echo "unknown option $1" >&2; exit 2 ;;
  esac
done
mode=${1:?mode: latency, replay or models}; shift || true
[[ $mode == models || $mode == sealed ]] && : "${ANTHROPIC_API_KEY:?set ANTHROPIC_API_KEY for a model pass}"
if [[ $mode == sealed ]]; then
  : "${BENCH_FRESH_DIR:?set BENCH_FRESH_DIR to the sealed folder}"
  python3 -I -B "$here/harness/fresh.py" verify "$BENCH_FRESH_DIR"
  pass_id=vm-$(date -u +%Y%m%dT%H%M%SZ)
fi
args=$(printf '%q ' "$@")
vx() { boxd machine exec "$name" -- bash -c "$1"; }
stop_bench() {
  vx "sudo pkill -KILL -u bench; for i in \$(seq 30); do pgrep -u bench >/dev/null || exit 0; sleep 1; done; echo 'bench processes still alive' >&2; exit 1"
}
fetched=0
fetch() {
  if [[ $mode == sealed ]]; then
    vx "sudo bash -c 'rm -rf /tmp/runs-out && mkdir -p /tmp/runs-out/runs && { cp /home/bench/runs/results-web.jsonl /tmp/runs-out/runs/ 2>/dev/null || true; } && tar -C /tmp/runs-out -czf /tmp/runs.tgz runs'" || return 1
  else
    vx "sudo test -d /home/bench/runs && sudo tar -C /home/bench --exclude=runs/fresh-staged --exclude=runs/fresh-out --exclude=runs/web-fresh -czf /tmp/runs.tgz runs" || return 1
  fi
  mkdir -p "$here/runs/boxd-$name"
  boxd machine cp "$name:/tmp/runs.tgz" "$here/runs/boxd-$name/runs.tgz"
  tar -xzf "$here/runs/boxd-$name/runs.tgz" -C "$here/runs/boxd-$name"
  fetched=1
}
cleanup() {
  [[ $mode == sealed ]] && { stop_bench 2>/dev/null || true; }
  [[ $fetched == 1 ]] || fetch 2>/dev/null || true
  rm -rf "$tmp"
  [[ $keep == 1 ]] || boxd machine remove "$name" --confirm || echo "could not remove $name: remove it by hand" >&2
}
tmp=$(mktemp -d)
boxd machine new "$name" --delete-after "${hours}h"
trap cleanup EXIT
boxd machine resize "$name" --vcpu "$vcpu" --memory "$memory" -y
tries=0
until [[ $(vx nproc 2>/dev/null) == "$vcpu" ]]; do
  (( ++tries > 60 )) && { echo "$name did not come back with $vcpu vCPUs after the resize" >&2; exit 1; }
  sleep 5
done
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
B=/home/bench/tabref-bench/bench
in_b="bash -c 'cd $B && exec \"\$@\"' _"
run="sudo -u bench -H env BENCH_TOOLS=/home/bench/bench-tools BENCH_RUNS=/home/bench/runs $tabref_env PATH=/usr/local/bin:/usr/bin:/bin $in_b"
case $mode in
  latency) vx "$run python3 -I latency/run.py --display xvfb --chrome $chrome $args" ;;
  replay) vx "$run python3 -I replay/run.py --display xvfb --chrome $chrome $args" ;;
  models)
    boxd machine exec --env "ANTHROPIC_API_KEY=$ANTHROPIC_API_KEY" "$name" -- bash -c \
      "sudo --preserve-env=ANTHROPIC_API_KEY -u bench -H env BENCH_TOOLS=/home/bench/bench-tools BENCH_RUNS=/home/bench/runs $tabref_env PATH=/usr/local/bin:/usr/bin:/bin $in_b python3 -I harness/run_web.py --display xvfb --chrome $chrome --host vm $args" ;;
  sealed)
    ( umask 077; tar -C "$BENCH_FRESH_DIR" -cf "$tmp/fresh.tar" . )
    vx 'rm -rf /tmp/sealed-in && mkdir -m 700 /tmp/sealed-in'
    boxd machine cp "$tmp/fresh.tar" "$name:/tmp/sealed-in/fresh.tar"; rm -f "$tmp/fresh.tar"
    vx "sudo install -o bench -m 600 /tmp/sealed-in/fresh.tar /home/bench/fresh.tar && rm -rf /tmp/sealed-in && sudo -u bench -H bash -c 'set -e; cd; rm -rf fresh; mkdir -m 700 fresh; tar -C fresh --no-same-owner -xf fresh.tar; chmod -R u+w fresh; rm -f fresh.tar'"
    vx "$run python3 -I -B harness/fresh.py verify /home/bench/fresh"
    rc=0
    boxd machine exec --env "ANTHROPIC_API_KEY=$ANTHROPIC_API_KEY" "$name" -- bash -c \
      "sudo --preserve-env=ANTHROPIC_API_KEY -u bench -H env BENCH_TOOLS=/home/bench/bench-tools BENCH_RUNS=/home/bench/runs $tabref_env PATH=/usr/local/bin:/usr/bin:/bin $in_b python3 -I harness/run_web.py --display xvfb --chrome $chrome --host vm $args --taskset fresh --fresh-dir /home/bench/fresh --fresh-remove-copy --fresh-pass-id $pass_id --out /home/bench/runs" || rc=$?
    stop_bench
    verdict=discard
    if [[ $rc != 0 ]]; then echo "the pass exited $rc: its rows are discarded" >&2
    elif python3 -I -B "$here/harness/fresh.py" verify "$BENCH_FRESH_DIR"; then verdict=commit
    else echo "fresh seal BROKEN after the pass (original folder): this pass's rows are discarded" >&2; fi
    vx "$run python3 -I -B harness/fresh.py $verdict /home/bench/runs $pass_id" || verdict=failed
    vx "sudo bash -c 'set -e; umask 077; d=/home/bench/runs/fresh-staged/$pass_id/sealed; mkdir -p \$d; tar -C \$d -czf /tmp/sealed.tgz .; chown \$SUDO_USER /tmp/sealed.tgz; rm -rf /home/bench/runs/fresh-staged /home/bench/fresh'" || echo "could not pack the sealed output" >&2
    ( umask 077; mkdir -p "$here/runs/fresh-out"; chmod 700 "$here/runs/fresh-out"
      boxd machine cp "$name:/tmp/sealed.tgz" "$here/runs/fresh-out/$pass_id.tgz" && chmod 600 "$here/runs/fresh-out/$pass_id.tgz" ) \
      || echo "could not copy the sealed output back" >&2
    echo "fresh pass $pass_id: staged rows: $verdict; sealed output in $here/runs/fresh-out/$pass_id.tgz (not for tuners)"
    [[ $verdict == commit ]] ;;
  *) echo "unknown mode $mode" >&2; exit 2 ;;
esac
fetch
echo "results in $here/runs/boxd-$name/runs (report: BENCH_RUNS=$here/runs/boxd-$name/runs python3 -I bench/report.py all)"
boxd machine usage "$name" 2>/dev/null | tail -5 || true
