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
# Cache native C/C++ compilation, including CXX-Qt's regenerated sources. HOST_*
# applies only when Cargo's host and target match, so cross compilers retain
# cc-rs's normal selection. Explicit compiler choices and flags take priority.
if command -v ccache >/dev/null 2>&1; then
    if [[ ! -v CC && ! -v HOST_CC ]]; then
        export HOST_CC="ccache cc"
    fi
    if [[ ! -v CXX && ! -v HOST_CXX ]]; then
        export HOST_CXX="ccache c++"
    fi
    # Docker's numeric build user may have no writable home directory. Keep the
    # cache outside Cargo targets so cleaning a target does not discard it.
    project_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
    export CCACHE_DIR=${CCACHE_DIR:-$project_root/build/ccache}
fi
# Cargo detects available CPUs; CI and constrained machines can set
# CARGO_BUILD_JOBS explicitly. Keep test execution concurrency independent.
# Refresh identity outside Cargo so unchanged sources do not force recompilation.
build_source_script=$(dirname "${BASH_SOURCE[0]}")/build-source-info.py
build_inputs=$(python3 "$build_source_script" --cargo-env)
if [[ -n "$build_inputs" ]]; then
    export NAGAMETV_BUILD_SNAPSHOT_ROOT="${build_inputs%%$'\n'*}"
    export NAGAMETV_BUILD_FINGERPRINT="${build_inputs#*$'\n'}"
fi
exec "$@"
