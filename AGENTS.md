# Repository instructions

Read and follow [the coding conventions](docs/coding-conventions.md) when adding
or changing code. Actively use enums for exclusive states and Typestate for
operations with prerequisites or ordering constraints. Keep constructors and
mutation APIs narrow enough that callers cannot bypass these guarantees.

Use the development and test commands in [docs/development.md](docs/development.md).
Preserve existing work in the shared workspace.

## Documentation updates

In OpenCode, delegate documentation creation and updates, including README files,
`docs/`, and contributor instructions, to the
[`writer` subagent](.opencode/agents/writer.md).
Give `writer` the requested scope, relevant implementation changes, and verification
results, then review its edits for accuracy. When acting as `writer`, perform the
documentation edits directly without delegating them again.

## Workshop hardware requirements

This project runs inside a container; display, GPU, camera and audio access must
not be assumed. Before using any hardware resource:

1. Detect the resource using environment variables, device paths or endpoints.
2. Validate that the detected resource functions.
3. If a required resource is unavailable, stop the dependent execution immediately.
4. Report exactly what is missing and why it is required.
5. Wait for the user to confirm or provide the resource before proceeding.

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
Do not connect automated tests to the host desktop. This mode does not validate physical monitors,
speakers or audio device latency, and must never fall back to software rendering
after a GPU failure.
