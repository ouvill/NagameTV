# Repository instructions

Read and follow [the coding conventions](docs/coding-conventions.md) when adding
or changing code. Actively use enums for exclusive states and Typestate for
operations with prerequisites or ordering constraints. Keep constructors and
mutation APIs narrow enough that callers cannot bypass these guarantees.

Use the development and test commands in [docs/development.md](docs/development.md).
Preserve existing work in the shared workspace.

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
rendering, private rootful Xwayland/Openbox and a private PulseAudio null sink.
Public GUI test scripts start and validate it automatically. Do not connect automated tests to
the host desktop or audio session. This mode does not validate physical monitors,
speakers or audio device latency, and must never fall back to software rendering
after a GPU failure.
