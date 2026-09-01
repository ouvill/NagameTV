cmake_minimum_required(VERSION 3.24)
project(mirakurun_viewer VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)

find_package(Qt6 6.4 REQUIRED COMPONENTS Core Gui Qml Quick OpenGL)

set(RUST_TARGET_DIR "${CMAKE_BINARY_DIR}/cargo")
set(RUST_LIBRARY "${RUST_TARGET_DIR}/release/libmirakurun_core.a")

add_custom_command(
    OUTPUT "${RUST_LIBRARY}"
    COMMAND ${CMAKE_COMMAND} -E env CARGO_TARGET_DIR=${RUST_TARGET_DIR}
            cargo build --manifest-path ${CMAKE_SOURCE_DIR}/rust/Cargo.toml --release
    DEPENDS rust/Cargo.toml rust/src/lib.rs
    WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}
    COMMENT "Building memory-safe Rust playback core"
    VERBATIM
)
add_custom_target(mirakurun_core ALL DEPENDS "${RUST_LIBRARY}")

qt_add_executable(mirakurun-viewer
    native/main.cpp
    native/player_controller.cpp
    native/player_controller.h
    native/video_item.cpp
    native/video_item.h
    native/rust_core.h
)

qt_add_qml_module(mirakurun-viewer
    URI MirakurunViewer
    VERSION 1.0
    QML_FILES qml/Main.qml
)

add_dependencies(mirakurun-viewer mirakurun_core)
target_include_directories(mirakurun-viewer PRIVATE native)
target_link_libraries(mirakurun-viewer PRIVATE
    Qt6::Core Qt6::Gui Qt6::Qml Qt6::Quick Qt6::OpenGL
    "${RUST_LIBRARY}" mpv pthread dl m
)

install(TARGETS mirakurun-viewer RUNTIME DESTINATION bin)
