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
    SOURCES ${SOURCES} rust/Cargo.toml rust/build.rs rust/qml/Main.qml rust/qml/SubtitleOverlay.qml rust/qml/ProgramGuide.qml
    VERBATIM)
install(PROGRAMS "${CMAKE_BINARY_DIR}/mirakurun-viewer" DESTINATION bin)
install(FILES packaging/linux/io.github.ouvill.litv.desktop DESTINATION share/applications)
install(FILES packaging/linux/io.github.ouvill.litv.svg DESTINATION share/icons/hicolor/scalable/apps)
install(FILES packaging/linux/io.github.ouvill.litv.metainfo.xml DESTINATION share/metainfo)
install(FILES assets/fonts/LICENSE-Rounded-Mplus-1m-for-ARIB.txt
    DESTINATION share/licenses/mirakurun-viewer)
install(FILES third_party/libaribcaption/LICENSE
    DESTINATION share/licenses/mirakurun-viewer RENAME libaribcaption.txt)
