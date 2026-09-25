cmake_minimum_required(VERSION 3.24)
project(nagametv LANGUAGES NONE)
option(NAGAMETV_DISTRIBUTION "Build an installable distribution (reject evaluation features)" OFF)
option(NAGAMETV_EVALUATION_LEGACY_COMMENTS "Local comparison of all legacy comment features; not for distribution" OFF)
set(COMMENT_EVALUATION_FEATURES "")
foreach(feature COMMENT_LIST WIDE_COMMENTS COLLISION_LAYOUT)
    option(NAGAMETV_EVALUATION_${feature} "Local comment evaluation only; not for distribution" OFF)
    if(NAGAMETV_EVALUATION_${feature} OR NAGAMETV_EVALUATION_LEGACY_COMMENTS)
        string(TOLOWER "${feature}" cargo_feature)
        string(REPLACE "_" "-" cargo_feature "${cargo_feature}")
        list(APPEND COMMENT_EVALUATION_FEATURES "evaluation-${cargo_feature}")
    endif()
endforeach()
if(NAGAMETV_DISTRIBUTION AND COMMENT_EVALUATION_FEATURES)
    message(FATAL_ERROR "Comment evaluation features cannot be used for distribution")
endif()
if(COMMENT_EVALUATION_FEATURES)
    message(WARNING "Evaluation-only comment build: do not distribute or install")
    install(CODE "message(FATAL_ERROR \"Evaluation-only comment builds cannot be installed\")")
endif()
set(CARGO_BUILD_FEATURES ${COMMENT_EVALUATION_FEATURES})
if(NAGAMETV_DISTRIBUTION)
    list(APPEND CARGO_BUILD_FEATURES distribution)
endif()
set(CARGO_FEATURE_ARGS "")
if(CARGO_BUILD_FEATURES)
    list(JOIN CARGO_BUILD_FEATURES "," CARGO_FEATURES_STRING)
    set(CARGO_FEATURE_ARGS --features "${CARGO_FEATURES_STRING}")
endif()
set(CARGO_TARGET_DIR "${CMAKE_BINARY_DIR}/cargo")
file(GLOB_RECURSE SOURCES CONFIGURE_DEPENDS "rust/src/*.rs" "rust/src/*.h")
add_custom_target(nagametv-build ALL
    COMMAND ${CMAKE_COMMAND} -E env CARGO_TARGET_DIR=${CARGO_TARGET_DIR}
            bash ${CMAKE_SOURCE_DIR}/scripts/with-build-lock.sh
            cargo build --manifest-path ${CMAKE_SOURCE_DIR}/rust/Cargo.toml --release --locked ${CARGO_FEATURE_ARGS}
    COMMAND ${CMAKE_COMMAND} -E copy_if_different
            ${CARGO_TARGET_DIR}/release/nagametv ${CMAKE_BINARY_DIR}/nagametv
    WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}
    SOURCES ${SOURCES} rust/Cargo.toml rust/build.rs rust/qml/Main.qml rust/qml/SubtitleOverlay.qml rust/qml/ProgramGuide.qml
    VERBATIM)
install(PROGRAMS "${CMAKE_BINARY_DIR}/nagametv" DESTINATION bin)
install(FILES packaging/linux/io.github.ouvill.nagametv.desktop DESTINATION share/applications)
install(FILES packaging/linux/io.github.ouvill.nagametv.svg DESTINATION share/icons/hicolor/scalable/apps)
install(FILES packaging/linux/io.github.ouvill.nagametv.metainfo.xml DESTINATION share/metainfo)
install(FILES assets/fonts/LICENSE-Rounded-Mplus-1m-for-ARIB.txt
    DESTINATION share/licenses/nagametv)
install(FILES third_party/libaribcaption/LICENSE
    DESTINATION share/licenses/nagametv RENAME libaribcaption.txt)

install(FILES third_party/tsreadex/License.txt
    DESTINATION share/licenses/nagametv RENAME tsreadex.txt)
