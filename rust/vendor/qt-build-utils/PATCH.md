# qt-build-utils 0.10.0 local build fix

Source: crates.io qt-build-utils 0.10.0, checksum
7c97911f105c362cbc035f631be7b43e44d9bca3a3aed2434260ba896f23d6b4.
Upstream: https://github.com/KDAB/cxx-qt/ (MIT OR Apache-2.0).
The package manifest and src tree are copied from that published crate.

In QtBuild::register_qml_module, run
qmltyperegistrar before qmlcachegen. A reused output directory otherwise
supplies the previous plugin.qmltypes to AOT compilation after Rust
invokables change, producing stale method indices and a startup SIGSEGV.
Preserve this ordering when updating the dependency; drop the local patch
once a verified upstream release contains the fix. No runtime dependency
or AOT-disable environment variable is introduced.

The QML type registrar also receives QtCore's installed metatypes JSON via
`--foreign-types`. Without it, `QML_FOREIGN(QSortFilterProxyModel)` loses its
QObject inheritance and sourceModel property in plugin.qmltypes. The path comes
from the selected installation's qtpaths, and a missing file fails the build.
The official Qt 6.8.3 SDK used by AppImage CI names this file
`qt6core_relwithdebinfo_metatypes.json`; the Linux Qt 6.10 packages use
`qt6core_metatypes.json`. Search `QT_INSTALL_ARCHDATA/metatypes` for the
unqualified filename first, then the standard CMake configuration suffixes
(`relwithdebinfo`, `release`, `minsizerel`, `debug`). Report the attempted paths
if no file is found. All paths come from the selected Qt installation.
Keep the foreign-type input mandatory so missing metadata cannot silently weaken
QML type checking.
