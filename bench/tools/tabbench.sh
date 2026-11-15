#!/usr/bin/env bash
# Model passes on a laptop run as a dedicated unprivileged user, `tabbench`, so the model can read nothing of the
# operator's: no ~/.config/gh, no ~/.claude, no other process's /proc/<pid>/environ, no operator display or D-Bus.
# Root is needed once, for setup; afterwards the operator only needs `sudo -n -u tabbench` (see below).
#
#   sudo bench/tools/tabbench.sh setup             once: create the user (no groups, home 700), then `sync`
#   bench/tools/tabbench.sh sync [TABREF_BIN]      copy this bench (and a tabref binary, default
#                                                  <main checkout>/target/release/tabref) to tabbench and install
#                                                  the pinned tools there as tabbench (bench/tools/install.sh)
#   <key on stdin> | bench/tools/tabbench.sh run <run_web.py args>
#                                                  a pass as tabbench; the key goes over stdin into that process's
#                                                  environment only (never a command line, file or log). With a
#                                                  terminal and no pipe it asks for the key without echo.
#   bench/tools/tabbench.sh results                copy tabbench's runs/ to bench/runs/tabbench/ for report.py
#
# Only setup needs root. sync, run and results need nothing but `sudo -n -u tabbench` for the operator, e.g. a
# sudoers drop-in `<operator> ALL=(tabbench) NOPASSWD: ALL`; every copy crosses the user boundary as a tar stream
# on a pipe, written by tabbench into its own home or by the operator into this checkout.
#
# run_web.py refuses to start unless it runs as tabbench (or on a VM), tabbench is in no admin group, and no other
# home under /home is readable by it. If yours is 755, make it private first: chmod 750 "$HOME" (your call).
set -euo pipefail
U=tabbench
H=/home/$U
here=$(cd "$(dirname "$0")/.." && pwd)
cmd=${1:?setup, sync, run or results}; shift || true
as_user() { sudo -n -u "$U" env -i HOME="$H" USER="$U" LOGNAME="$U" LANG=C.UTF-8 PATH=/usr/bin:/bin "$@"; }
need_user() {
  as_user true 2>/dev/null || { echo "cannot run as $U without a password (sudo -n -u $U); see the header" >&2; exit 2; }
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
    exec "$0" sync "$@"
    ;;
  sync)
    if [[ $(id -u) == 0 ]]; then
      op=${SUDO_USER:-}
      [[ -n $op && $op != root ]] || { echo "run sync as the operator, not root" >&2; exit 2; }
      exec runuser -u "$op" -- "$0" sync "$@"
    fi
    need_user
    common=$(git -C "$here" rev-parse --path-format=absolute --git-common-dir 2>/dev/null || true)
    tb=${1:-${common:+$(dirname "$common")/target/release/tabref}}
    tar -C "$here/.." --exclude='bench/runs' --exclude='__pycache__' -cf - bench \
      | as_user bash -c 'set -e; rm -rf "$HOME/tabref.new"; mkdir -p "$HOME/tabref.new" "$HOME/bin"
          tar -C "$HOME/tabref.new" --no-same-owner -xf -
          rm -rf "$HOME/tabref"; mv "$HOME/tabref.new" "$HOME/tabref"'
    if [[ -n $tb && -x $tb ]]; then
      as_user bash -c 'set -e; cat > "$HOME/bin/tabref.new"; chmod 755 "$HOME/bin/tabref.new"
          mv "$HOME/bin/tabref.new" "$HOME/bin/tabref"' < "$tb"
      echo "tabref: $tb -> $H/bin/tabref"
    else echo "tabref: no binary (${tb:-none}); tabref rows will be skipped"; fi
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
    printf '%s\n' "$key" | as_user TABBENCH_INNER=1 bash "$H/tabref/bench/tools/tabbench.sh" run "$@"
    ;;
  results)
    [[ $(id -u) != 0 ]] || { echo "run results as the operator, not root" >&2; exit 2; }
    need_user
    mkdir -p "$here/runs/tabbench"
    as_user bash -c '[[ -d "$HOME/runs" ]] || exit 0; tar -C "$HOME/runs" -cf - .' \
      | tar -C "$here/runs/tabbench" --no-same-owner --no-same-permissions -xf -
    echo "report: BENCH_RUNS=$here/runs/tabbench python3 -I bench/report.py models --baseline"
    ;;
  *) echo "unknown command $cmd" >&2; exit 2 ;;
esac
