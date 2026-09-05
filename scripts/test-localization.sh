#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
test_dir=$(mktemp -d)
qt_bins=$(qtpaths6 --query QT_HOST_BINS)
qt_libexec=$(qtpaths6 --query QT_HOST_LIBEXECS)
"$qt_bins/lrelease" translations/app_ja.ts -qm "$test_dir/ja.qm"
# Build a test-only resource manifest with the production resource alias.
printf '<RCC><qresource prefix="/i18n"><file alias="ja.qm">%s/ja.qm</file></qresource></RCC>\n' "$test_dir" > "$test_dir/translations.qrc"
"$qt_libexec/rcc" "$test_dir/translations.qrc" -o "$test_dir/translations.cpp"
${CXX:-c++} -std=c++17 -fPIC -Irust/src tests/localization_smoke.cpp "$test_dir/translations.cpp" \
    $(pkg-config --cflags --libs Qt6Core Qt6Qml) -o "$test_dir/localization-test"
"$test_dir/localization-test"
