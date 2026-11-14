#!/bin/bash
# Linux port of heldout/arms/logwrap.zsh: runs the real tool, prints its stdout, appends one line per call:
#   t0<TAB>t1<TAB>rc<TAB>stdout_bytes<TAB>args   to $AD_LOG.
real=$1; shift
log=${AD_LOG:-}
tmp=$(mktemp "${TMPDIR:-/tmp}/ho-out.XXXXXX") || exit 70
t0=$(date +%s.%N)
"$real" "$@" > "$tmp"
rc=$?
t1=$(date +%s.%N)
cat "$tmp"
n=$(stat -c %s "$tmp")
rm -f "$tmp"
[[ -n $log ]] && printf '%s\t%s\t%s\t%s\t%s\n' "$t0" "$t1" "$rc" "$n" "$(printf '%q ' "$@")" >> "$log"
exit $rc
