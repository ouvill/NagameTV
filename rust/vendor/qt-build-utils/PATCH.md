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
This change is verified on Linux with Qt 6.10; other Qt installations must also
provide `QT_INSTALL_ARCHDATA/metatypes/qt6core_metatypes.json`.
