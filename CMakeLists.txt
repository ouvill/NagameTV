cmake_minimum_required(VERSION 3.24)
project(mirakurun_viewer VERSION 0.2.0 LANGUAGES NONE)

set(CARGO_TARGET_DIR "${CMAKE_BINARY_DIR}/cargo")
set(CARGO_BINARY "${CARGO_TARGET_DIR}/release/mirakurun-viewer")
set(OUTPUT_BINARY "${CMAKE_BINARY_DIR}/mirakurun-viewer")
file(GLOB ICON_ASSETS CONFIGURE_DEPENDS "${CMAKE_SOURCE_DIR}/assets/icons/*.svg")

add_custom_command(
    OUTPUT "${OUTPUT_BINARY}"
    COMMAND ${CMAKE_COMMAND} -E env CARGO_TARGET_DIR=${CARGO_TARGET_DIR}
            cargo build --manifest-path ${CMAKE_SOURCE_DIR}/rust/Cargo.toml --release
    COMMAND ${CMAKE_COMMAND} -E copy_if_different "${CARGO_BINARY}" "${OUTPUT_BINARY}"
    DEPENDS
        rust/Cargo.toml rust/Cargo.lock rust/build.rs
        rust/src/main.rs rust/src/player.rs rust/src/playback.rs rust/src/qt_helpers.h
        qml/Main.qml
        ${ICON_ASSETS} assets/icons/LICENSE-Lucide.txt
    WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}
    COMMENT "Building Rust/CXX-Qt Mirakurun Viewer"
    VERBATIM
)

add_custom_target(mirakurun-viewer-build ALL DEPENDS "${OUTPUT_BINARY}")
install(PROGRAMS "${OUTPUT_BINARY}" DESTINATION bin)
