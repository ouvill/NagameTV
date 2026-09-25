#!/usr/bin/env bash
# Serialize supported builds and tests across worktrees belonging to this user.
set -euo pipefail
if (( $# == 0 )); then
    echo "Usage: scripts/with-build-lock.sh COMMAND [ARG ...]" >&2
    exit 2
fi
lock_dir=${NAGAMETV_BUILD_LOCK_DIR:-/tmp/nagametv-build-$UID}
(
    umask 077
    mkdir -p "$lock_dir"
)
lock_file=$lock_dir/lock
# A marker alone is insufficient: the open lock must have survived inheritance.
if [[ ${NAGAMETV_BUILD_LOCK_HELD:-} == "$lock_file" && /proc/$$/fd/9 -ef "$lock_file" ]]; then
    exec "$@"
fi
exec 9>>"$lock_file"
if ! flock --exclusive --nonblock 9; then
    echo "Waiting for another NagameTV build/test: $lock_file" >&2
    flock --exclusive 9
fi
export NAGAMETV_BUILD_LOCK_HELD=$lock_file
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
exec "$@"
