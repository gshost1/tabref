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
#   bench/tools/tabbench.sh results                copy tabbench's runs/ to bench/runs/tabbench/ for report.py
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
cmd=${1:?setup, sync, run or results}; shift || true
AS=${TABBENCH_AS:-/usr/local/bin/as-tabbench}
[[ $AS == /* ]] || { echo "TABBENCH_AS must be an absolute path" >&2; exit 2; }
as_raw() { ( cd / && "$AS" env -i HOME="$H" USER="$U" LOGNAME="$U" LANG=C.UTF-8 PATH=/usr/bin:/bin "$@" ) \
  > >(cat) 2> >(cat >&2); }
as_user() { if [[ -t 0 ]]; then as_raw "$@" < /dev/null; else as_raw "$@"; fi; }
need_user() {
  local err
  err=$(as_user true 2>&1) || { echo "cannot run as $U through $AS: $err (see the header)" >&2; exit 2; }
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
    need_user
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
        python3 -I bench/harness/run_web.py "$@" < /dev/null
    fi
    if [[ -t 0 ]]; then IFS= read -rs -p "Anthropic API key for the bench: " key; echo; else IFS= read -r key; fi
    [[ -n $key ]] || { echo "no key" >&2; exit 2; }
    need_user
    sync_files "${TABREF_BIN:-$(default_bin)}"
    rc=0
    printf '%s\n' "$key" | as_raw TABBENCH_INNER=1 bash "$H/tabref/bench/tools/tabbench.sh" run "$@" || rc=$?
    unset key
    as_user bash -c 'kill -9 -1' 2>/dev/null || true
    exit "$rc"
    ;;
  results)
    [[ $(id -u) != 0 ]] || { echo "run results as the operator, not root" >&2; exit 2; }
    need_user
    mkdir -p "$here/runs"
    tmp=$(mktemp -d "$here/runs/.tabbench.XXXXXX")
    as_user bash -c '[[ -d "$HOME/runs" ]] || exit 0; tar -C "$HOME/runs" -cf - .' \
      | python3 -I -c 'import sys, tarfile
def keep(m, path):
    return tarfile.data_filter(m, path) if m.isfile() or m.isdir() else None
with tarfile.open(fileobj=sys.stdin.buffer, mode="r|") as t:
    t.extractall(sys.argv[1], filter=keep)' "$tmp" || { rm -rf "$tmp"; exit 1; }
    find "$tmp" ! -type f ! -type d -delete
    rm -rf "$here/runs/tabbench"; mv "$tmp" "$here/runs/tabbench"
    echo "report: BENCH_RUNS=$here/runs/tabbench python3 -I bench/report.py models --baseline"
    ;;
  *) echo "unknown command $cmd" >&2; exit 2 ;;
esac
