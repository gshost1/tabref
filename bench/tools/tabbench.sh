#!/usr/bin/env bash
# Model passes on a laptop run as a dedicated unprivileged user, `tabbench`, so the model can read nothing of the
# operator's: no ~/.config/gh, no ~/.claude, no other process's /proc/<pid>/environ, no operator display or D-Bus.
# Root is needed once, for setup; afterwards the operator only needs the as-tabbench wrapper (see below).
#
#   sudo bench/tools/tabbench.sh setup             once: create the user (no groups, home 700), then `sync`
#   bench/tools/tabbench.sh sync [TABREF_BIN]      copy this bench (and a tabref binary, default
#                                                  <main checkout>/target/release/tabref) to tabbench and install
#                                                  the pinned tools there as tabbench (bench/tools/install.sh)
#   <key on stdin> | bench/tools/tabbench.sh run <run_web.py args>
#                                                  re-sync the bench files and binary (so a pass never runs code an
#                                                  earlier pass could have edited), then a pass as tabbench; the key goes over stdin into that process's
#                                                  environment only (never a command line, file or log). With a
#                                                  terminal and no pipe it asks for the key without echo.
#   bench/tools/tabbench.sh smoke [--taskset fresh|dev] [run_web.py args]
#                                                  no model, no key: as tabbench, start the set's site, reset every
#                                                  task for run 0 and count verifiers that reject an empty answer
#   bench/tools/tabbench.sh results                copy tabbench's runs/ to bench/runs/tabbench/ for report.py
#                                                  (never the fresh staging or sealed output)
#
# sync, run and smoke take a lock (flock on $TABBENCH_LOCK, default $XDG_RUNTIME_DIR/tabbench.lock, on the operator's
# side so the cleanup below cannot drop it) and refuse while tabbench has any process. One EXIT trap (also on INT,
# TERM, HUP) kills every tabbench process first, so no claude outlives the job holding the key, then removes the
# fresh copy and any fresh staging. A clean start removes those leftovers too.
#
# --taskset fresh (run and smoke): the sealed folder (--fresh-dir <dir> or BENCH_FRESH_DIR) is seal-checked here,
# copied into tabbench's home as a tar stream ($HOME/fresh, made owner-writable there so a read-only original's copy
# can still be deleted; the seal covers content, not modes), seal-checked again there, and handed to run_web.py as a
# disposable copy (--fresh-remove-copy): run_web.py deletes it once its module is loaded and its server is up. After
# the pass the operator's original is seal-checked again; only then are the pass's staged rows committed to
# results-web.jsonl (else discarded). The sealed output (answers, verifier text, transcripts) is moved out of
# tabbench's home into this checkout's bench/runs/fresh-out/<pass id>/ (mode 700), so no later model can read it.
#
# --taskset dev (run and smoke): the unsealed tuning set in bench/dev/. It is tracked, so sync_files copies it with
# the rest of the bench (commit it first: only `git ls-files` reach tabbench); no seal check, no staging, rows go
# straight to results-web.jsonl like heldout ones and `results` copies them back.
#
# Only setup needs root. sync, run and results run as the operator and reach tabbench only through a root-owned
# wrapper, /usr/local/bin/as-tabbench (`exec sudo -n -u tabbench "$@"`, with a sudoers drop-in
# `<operator> ALL=(tabbench) NOPASSWD: ALL`); TABBENCH_AS names another wrapper. Every copy crosses the user
# boundary as a tar stream on a pipe, written by tabbench into its own home or by the operator into this checkout.
#
# run_web.py refuses to start unless it runs as tabbench (or on a VM), tabbench is in no admin group, and no other
# home under /home is readable by it. If yours is 755, make it private first: chmod 750 "$HOME" (your call).
set -euo pipefail
U=tabbench
H=/home/$U
here=$(cd "$(dirname "$0")/.." && pwd)
cmd=${1:?setup, sync, run, smoke or results}; shift || true
AS=${TABBENCH_AS:-/usr/local/bin/as-tabbench}
[[ $AS == /* ]] || { echo "TABBENCH_AS must be an absolute path" >&2; exit 2; }
LOCK=${TABBENCH_LOCK:-${XDG_RUNTIME_DIR:-$HOME/.cache}/tabbench.lock}
as_raw() { ( cd / && "$AS" env -i HOME="$H" USER="$U" LOGNAME="$U" LANG=C.UTF-8 PATH=/usr/bin:/bin "$@" ) \
  > >(cat) 2> >(cat >&2); }
as_user() { if [[ -t 0 ]]; then as_raw "$@" < /dev/null; else as_raw "$@"; fi; }
need_user() {
  local err
  err=$(as_user true 2>&1) || { echo "cannot run as $U through $AS: $err (see the header)" >&2; exit 2; }
}
untar_to() {
  python3 -I -B -c 'import sys, tarfile
def keep(m, path):
    return tarfile.data_filter(m, path) if m.isfile() or m.isdir() else None
with tarfile.open(fileobj=sys.stdin.buffer, mode="r|") as t:
    t.extractall(sys.argv[1], filter=keep)' "$1" || return 1
  find "$1" ! -type f ! -type d -delete
}
stop_all() { as_user bash -c 'kill -9 -1' 2>/dev/null || true; }
fresh_out() {
  as_user bash -c 'chmod -R u+w "$HOME/fresh" "$HOME/fresh.new" 2>/dev/null
      rm -rf "$HOME/fresh" "$HOME/fresh.new" "$HOME/runs/fresh-staged" "$HOME/runs-smoke"' 2>/dev/null || true
}
cleanup() { stop_all; fresh_out; }
begin() {
  need_user
  mkdir -p "$(dirname "$LOCK")"
  exec 9>>"$LOCK"
  flock -n 9 || { echo "another tabbench job holds $LOCK; refusing" >&2; exit 75; }
  if pgrep -u "$U" >/dev/null; then
    echo "$U has running processes (a pass in progress, or left over); refusing. To clear leftovers: $AS bash -c 'kill -9 -1'" >&2
    exit 75
  fi
  trap cleanup EXIT; trap 'exit 130' INT; trap 'exit 143' TERM; trap 'exit 129' HUP
  fresh_out
}
sync_files() {
  local tb=$1
  git -C "$here/.." ls-files -z bench | grep -zv '^bench/runs/' \
    | tar -C "$here/.." --null -T - -cf - \
    | as_user bash -c 'set -e; rm -rf "$HOME/tabref.new"; mkdir -p "$HOME/tabref.new" "$HOME/bin"
        tar -C "$HOME/tabref.new" --no-same-owner -xf -
        rm -rf "$HOME/tabref"; mv "$HOME/tabref.new" "$HOME/tabref"'
  if [[ -n $tb && -x $tb ]]; then
    as_user bash -c 'set -e; cat > "$HOME/bin/tabref.new"; chmod 755 "$HOME/bin/tabref.new"
        mv "$HOME/bin/tabref.new" "$HOME/bin/tabref"' < "$tb"
    echo "tabref: $tb -> $H/bin/tabref"
  else echo "tabref: no binary (${tb:-none}); tabref rows will be skipped"; fi
}
FRESH=0
FRESH_SRC=${BENCH_FRESH_DIR:-}
PASS_ID=""
ARGS=()
parse_set() {
  local want="" flag=0
  while (($#)); do
    case $1 in
      --taskset) want=${2:-}; ARGS+=("$1" "${2:-}"); shift 2 || shift ;;
      --taskset=*) want=${1#*=}; ARGS+=("$1"); shift ;;
      --fresh-dir) FRESH_SRC=${2:-}; flag=1; shift 2 || shift ;;
      --fresh-dir=*) FRESH_SRC=${1#*=}; flag=1; shift ;;
      --fresh-remove-copy|--fresh-pass-id|--fresh-pass-id=*) echo "$1 is set by tabbench.sh itself" >&2; exit 2 ;;
      *) ARGS+=("$1"); shift ;;
    esac
  done
  if [[ $want == fresh ]]; then
    FRESH=1
    [[ -n $FRESH_SRC ]] || { echo "--taskset fresh needs --fresh-dir <dir> or BENCH_FRESH_DIR" >&2; exit 2; }
    ARGS+=(--fresh-dir "$H/fresh" --fresh-remove-copy)
  elif ((flag)); then
    echo "warning: --fresh-dir is ignored without --taskset fresh" >&2
  fi
}
fresh_in() {
  python3 -I -B "$here/harness/fresh.py" verify "$FRESH_SRC"
  tar -C "$FRESH_SRC" -cf - . \
    | as_user bash -c 'set -e; rm -rf "$HOME/fresh.new" "$HOME/fresh"; mkdir -m 700 "$HOME/fresh.new"
        tar -C "$HOME/fresh.new" --no-same-owner -xf -; chmod -R u+w "$HOME/fresh.new"; mv "$HOME/fresh.new" "$HOME/fresh"'
  as_user python3 -I -B "$H/tabref/bench/harness/fresh.py" verify "$H/fresh"
}
fresh_finish() {
  local verdict=discard out="$here/runs/fresh-out"
  if python3 -I -B "$here/harness/fresh.py" verify "$FRESH_SRC"; then verdict=commit
  else echo "fresh seal BROKEN after the pass (original folder): this pass's rows are discarded" >&2; fi
  as_user python3 -I -B "$H/tabref/bench/harness/fresh.py" "$verdict" "$H/runs" "$PASS_ID" || verdict=failed
  mkdir -p "$out"; chmod 700 "$out"; rm -rf "$out/$PASS_ID.part"; mkdir "$out/$PASS_ID.part"
  if as_user bash -c 'd="$HOME/runs/fresh-staged/$1/sealed"; mkdir -p "$d"; tar -C "$d" -cf - .' _ "$PASS_ID" \
      | untar_to "$out/$PASS_ID.part"; then
    mv "$out/$PASS_ID.part" "$out/$PASS_ID"; echo "fresh sealed output: $out/$PASS_ID (not for tuners)"
  else echo "could not move the sealed output out of $H; it is removed with the staging" >&2; fi
  as_user bash -c 'rm -rf "$HOME/runs/fresh-staged/$1"' _ "$PASS_ID" || true
  [[ $verdict == commit ]]
}
default_bin() {
  local common
  common=$(git -C "$here" rev-parse --path-format=absolute --git-common-dir 2>/dev/null || true)
  echo "${common:+$(dirname "$common")/target/release/tabref}"
}
case $cmd in
  setup)
    [[ $(id -u) == 0 ]] || { echo "run setup with sudo" >&2; exit 2; }
    id "$U" >/dev/null 2>&1 || useradd --create-home --shell /bin/bash --user-group "$U"
    chmod 700 "$H"
    extra=$(id -nG "$U" | tr ' ' '\n' | grep -vx "$U" || true)
    [[ -z $extra ]] || { echo "$U is in extra groups: $extra; remove them" >&2; exit 1; }
    for d in /home/*; do
      [[ $d == "$H" || ! -d $d ]] && continue
      if runuser -u "$U" -- test -r "$d" -a -x "$d"; then
        echo "note: $U can read $d; run_web.py will refuse until it cannot (e.g. chmod 750 $d)"
      fi
    done
    op=${SUDO_USER:-}
    [[ -n $op && $op != root ]] || { echo "run setup through sudo from the operator's account" >&2; exit 2; }
    if [[ ! -x /usr/local/bin/as-tabbench ]]; then
      printf '#!/bin/sh\nexec sudo -n -u %s "$@"\n' "$U" > /usr/local/bin/as-tabbench
      chown root:root /usr/local/bin/as-tabbench; chmod 755 /usr/local/bin/as-tabbench
    fi
    if [[ ! -e /etc/sudoers.d/$U ]]; then
      t=$(mktemp); printf '%s ALL=(%s) NOPASSWD: ALL\n' "$op" "$U" > "$t"
      visudo -cf "$t" >/dev/null || { rm -f "$t"; echo "sudoers rule failed visudo" >&2; exit 1; }
      install -o root -g root -m 440 "$t" "/etc/sudoers.d/$U"; rm -f "$t"
    fi
    exec "$0" sync "$@"
    ;;
  sync)
    if [[ $(id -u) == 0 ]]; then
      op=${SUDO_USER:-}
      [[ -n $op && $op != root ]] || { echo "run sync as the operator, not root" >&2; exit 2; }
      exec runuser -u "$op" -- "$0" sync "$@"
    fi
    begin
    sync_files "${1:-$(default_bin)}"
    as_user BENCH_TOOLS="$H/bench-tools" bash "$H/tabref/bench/tools/install.sh"
    ;;
  run)
    if [[ ${TABBENCH_INNER:-} == 1 ]]; then
      IFS= read -r ANTHROPIC_API_KEY || true
      [[ -n $ANTHROPIC_API_KEY ]] || { echo "no key on stdin" >&2; exit 2; }
      export ANTHROPIC_API_KEY
      cd "$H/tabref"
      exec env -u TABBENCH_INNER BENCH_TOOLS="$H/bench-tools" BENCH_RUNS="$H/runs" TABREF_BIN="$H/bin/tabref" \
        python3 -I -B bench/harness/run_web.py "$@" < /dev/null
    fi
    [[ $(id -u) != 0 ]] || { echo "run passes as the operator, not root" >&2; exit 2; }
    parse_set "$@"
    if [[ -t 0 ]]; then IFS= read -rs -p "Anthropic API key for the bench: " key || true; echo; else IFS= read -r key || true; fi
    [[ -n $key ]] || { echo "no key" >&2; exit 2; }
    begin
    sync_files "${TABREF_BIN:-$(default_bin)}"
    if ((FRESH)); then
      PASS_ID=fresh-$(date -u +%Y%m%dT%H%M%SZ)
      ARGS+=(--fresh-pass-id "$PASS_ID")
      fresh_in
    fi
    rc=0
    printf '%s\n' "$key" | as_raw TABBENCH_INNER=1 bash "$H/tabref/bench/tools/tabbench.sh" run "${ARGS[@]}" || rc=$?
    unset key
    stop_all
    if ((FRESH)); then fresh_finish || rc=3; fi
    exit "$rc"
    ;;
  smoke)
    [[ $(id -u) != 0 ]] || { echo "run smoke as the operator, not root" >&2; exit 2; }
    parse_set "$@"
    begin
    sync_files "${TABREF_BIN:-$(default_bin)}"
    ((FRESH)) && fresh_in
    rc=0
    as_user bash -c 'cd "$HOME/tabref" && exec env BENCH_TOOLS="$HOME/bench-tools" BENCH_RUNS="$HOME/runs-smoke" \
        python3 -I -B bench/harness/run_web.py --smoke "$@"' _ "${ARGS[@]}" || rc=$?
    stop_all
    if ((FRESH)); then python3 -I -B "$here/harness/fresh.py" verify "$FRESH_SRC" || rc=3; fi
    exit "$rc"
    ;;
  results)
    [[ $(id -u) != 0 ]] || { echo "run results as the operator, not root" >&2; exit 2; }
    need_user
    mkdir -p "$here/runs"
    tmp=$(mktemp -d "$here/runs/.tabbench.XXXXXX")
    as_user bash -c '[[ -d "$HOME/runs" ]] || exit 0
        tar -C "$HOME/runs" --exclude=./fresh-staged --exclude=./fresh-out --exclude=./web-fresh -cf - .' \
      | untar_to "$tmp" || { rm -rf "$tmp"; exit 1; }
    rm -rf "$here/runs/tabbench"; mv "$tmp" "$here/runs/tabbench"
    echo "report: BENCH_RUNS=$here/runs/tabbench python3 -I bench/report.py models --baseline [--taskset fresh]"
    ;;
  *) echo "unknown command $cmd" >&2; exit 2 ;;
esac
