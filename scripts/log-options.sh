# Sourced by launchers; no device access or process startup here.
parse_log_options() {
  local mode="$1" arg key value
  shift
  local diag_default=1 gc_default=0 heap_default=0
  if [[ "$mode" == profile ]]; then gc_default=1; heap_default=1; fi
  local diag="${MIRAKURUN_DIAGNOSTICS-$diag_default}"
  local gc="${MIRAKURUN_GC_LOG-$gc_default}"
  local heap="${MIRAKURUN_HEAPTRACK-$heap_default}"
  local diag_explicit="${MIRAKURUN_DIAGNOSTICS+x}" isolated=0
  viewer_args=()
  print_log_settings=0
  for arg in "$@"; do
    case "$arg" in
      --diagnostics=*|--gc-log=*|--heaptrack=*)
        key="${arg%%=*}"; value="${arg#*=}"
        case "$value" in 0|1) ;; *) echo "$key requires 0 or 1" >&2; return 2;; esac
        case "$key" in
          --diagnostics) diag="$value"; diag_explicit=x;;
          --gc-log) gc="$value";;
          --heaptrack) heap="$value";;
        esac;;
      --diagnostics|--gc-log|--heaptrack)
        echo "$arg requires =0 or =1" >&2; return 2;;
      --print-log-settings) print_log_settings=1;;
      *) viewer_args+=("$arg"); [[ "$arg" == --features=* ]] && isolated=1;;
    esac
  done
  if [[ "$mode" == run && "$isolated" == 1 && -z "$diag_explicit" ]]; then diag=0; fi
  for value in "$diag" "$gc" "$heap"; do
    case "$value" in 0|1) ;; *) echo 'Logging environment values must be 0 or 1' >&2; return 2;; esac
  done
  export MIRAKURUN_DIAGNOSTICS="$diag" MIRAKURUN_GC_LOG="$gc" MIRAKURUN_HEAPTRACK="$heap"
}
show_log_settings() {
  printf 'diagnostics=%s gc-log=%s heaptrack=%s\n' "$MIRAKURUN_DIAGNOSTICS" "$MIRAKURUN_GC_LOG" "$MIRAKURUN_HEAPTRACK"
}
