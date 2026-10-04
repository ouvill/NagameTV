# Repository instructions

## Scope and execution

Carry requested changes through implementation and relevant verification.
Choose routine, reversible implementation details from the existing code and
conventions. Local edits and documented tests within the request do not need
separate approval, subject to the hardware requirements below.

Ask when missing information would materially change the result and cannot be
inferred from context. Continue independent, authorized work while awaiting an
answer. User instructions take precedence over local skill guidelines. If a
repository or skill rule blocks a step, link the file, quote the rule and explain
which step is blocked. Report results, relevant checks and remaining blockers
concisely.

## Code and verification

For code changes, follow the relevant sections of
[the coding conventions](docs/coding-conventions.md). Use
[the development guide](docs/development.md) for build and test commands,
[the UI design guide](docs/ui-design.md) for UI changes and
[the architecture guide](docs/architecture.md) for ownership or service boundaries.
Read the sections needed for the task; a small edit does not require a full
repository or documentation review.

Use trunk-based development with short-lived branches and small, reviewable
integrations. Gate unfinished user paths behind default-off runtime feature
flags at the Rust ownership boundary. Follow the flag lifecycle and verification
rules in [the development guide](docs/development.md#トランクベース開発とフィーチャーフラグ).

Actively use enums for exclusive states and Typestate for
operations with prerequisites or ordering constraints. Keep constructors and
mutation APIs narrow enough that callers cannot bypass these guarantees.

Use clap for startup argument parsing, thiserror for Rust error types, and sqlx
checked queries for SQLite access. Keep shared native Qt services in `qt` and
QObject presentation APIs in their adapters. Qt/QML form the presentation layer;
keep authoritative application state and use-case decisions in Qt-independent
Rust types. See the conventions for ownership, offline SQL metadata and validation.

Report operational failures to terminal stderr through `tracing`, including the
operation and underlying causes, even when the UI or a file also reports them.
Log when handling a failure, not when rendering retained error state. Do not
silently discard errors with `let _ =`, `.ok()` or fallback values; propagate them
or record them. Document intentional exceptions for normal cancellation, absent
optional data and shutdown races. Never log credentials or comment drafts.

Select checks for the changed behavior and complete the required checks described
in the conventions and development guide. Once they pass, repeat or broaden them
only for further changes, failures or unresolved concerns. For prose-only changes,
check the wording, links and diff; application builds and GUI tests are unnecessary.

Use `python3 scripts/test.py` for tests (`--list` lists suites). It runs Rust tests
with nextest and shares a build/test lock with CMake and the Qt test scripts.
Wrap direct Cargo diagnostics, including Clippy, in
`bash scripts/build/with-build-lock.sh COMMAND ...` so they do not overlap a test run.

## Development scripts

Use [the script guide](scripts/README.md) for public commands and placement.
Keep ordinary build helpers in `scripts/build`, test execution in
`scripts/testing`, distribution automation in `scripts/packaging`, and manual
development tools in `scripts/dev`. Script regression tests belong in
`tests/tooling`; fixture generators stay with the data in `tests/fixtures`.

Extend `scripts/test.py` and its suite definitions for new tests instead of
adding per-suite launch scripts. Keep hardware requirements explicit and use
the shared validated GUI session. Build helpers must not depend on test or
diagnostic modules. Prefer Python for orchestration and data handling; retain
short shell wrappers for OS commands. Use importable Python module names and
`python3 -m scripts.<area>.<module>` from the repository root.

Before keeping a new script, identify its recurring use, caller, prerequisites
and output in the script guide or the relevant task documentation. Keep
one-off investigation code in ignored work directories unless it becomes a
documented, reusable tool or regression test. Update callers, CI, Workshop,
packaging source lists and documentation together when moving an entry point.

## Workshop hardware requirements

This project runs inside a container; display, GPU, camera and audio access must
not be assumed. Before using any hardware resource:

1. Detect the resource using environment variables, device paths or endpoints.
2. Validate that the detected resource functions.
3. If a required resource is unavailable, stop the dependent execution immediately.
4. Report exactly what is missing and why it is required.
5. Wait for the user to confirm or provide the resource before resuming that execution.

Do not modify container configuration or silently substitute headless, software
rendering or stub implementations for missing hardware. Favor deterministic
failure over silent degradation. Tests documented as hardware-free must remain
hardware-free.

The checked-in [isolated GUI test environment](docs/gui-test-environment.md) is
an explicitly configured display/audio test mode: Weston headless with real GPU
rendering, private rootful Xwayland/Openbox and a per-run PipeWire null sink.
Workshop starts PipeWire and pipewire-pulse as environment services; tests never
start or stop an audio server. On native Linux (including Fedora), GUI tests may
use the existing local pipewire-pulse socket selected by PULSE_SERVER or the
original XDG_RUNTIME_DIR. They must validate the server, create and explicitly
target their own virtual sink/monitor, and remove only their own resources.
Never change global audio defaults or other clients' streams, volumes or outputs.
Public GUI test scripts start and validate their private display automatically.
Use these public scripts for the requested verification without a separate
permission step; their resource checks remain mandatory.
Do not connect automated tests to the host desktop. This mode does not validate physical monitors,
speakers or audio device latency, and must never fall back to software rendering
after a GPU failure.
