cmake_minimum_required(VERSION 3.24)
project(mirakurun_viewer_minimal LANGUAGES NONE)
set(CARGO_TARGET_DIR "${CMAKE_BINARY_DIR}/cargo")
file(GLOB_RECURSE SOURCES CONFIGURE_DEPENDS "rust/src/*.rs" "rust/src/*.h")
add_custom_target(mirakurun-viewer-build ALL
    COMMAND ${CMAKE_COMMAND} -E env CARGO_TARGET_DIR=${CARGO_TARGET_DIR}
            cargo build --manifest-path ${CMAKE_SOURCE_DIR}/rust/Cargo.toml --release --locked
    COMMAND ${CMAKE_COMMAND} -E copy_if_different
            ${CARGO_TARGET_DIR}/release/mirakurun-viewer ${CMAKE_BINARY_DIR}/mirakurun-viewer
    WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}
    SOURCES ${SOURCES} rust/Cargo.toml rust/build.rs qml/Main.qml
    VERBATIM)
install(PROGRAMS "${CMAKE_BINARY_DIR}/mirakurun-viewer" DESTINATION bin)
