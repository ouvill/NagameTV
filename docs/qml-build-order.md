# Qt型情報変更後の増分ビルド

qt-build-utils 0.10.0ではregister_qml_moduleがqmlcachegenを実行した後に
qmltyperegistrarを実行していた。Rustにwatch_program invokableを追加した際、
前のplugin.qmltypesが残る出力ディレクトリーで、Main.qmlのAOTコードが旧メソッド番号34を
使用し、現行のobserve_pointerの番号35と不一致になった。
実アプリは起動直後、QMetaObject::methodOffset→method→QML AOT lookupでSIGSEGVになった。
GDBの該当位置は生成Main.qml.cppのinitCallObjectPropertyLookup(154, ..., 34)。

rust/vendor/qt-build-utilsは公開0.10.0のmanifestとsrcを保持し、型登録をAOT生成より前に
移動する修正だけを適用した。Cargo.tomlのpatchでビルド依存に使う。上流ライセンスと
出典・チェックサム・差分は同ディレクトリーに置く。レジストリーの共有キャッシュは編集しない。
AOTの無効化、実行時環境変数、ユーザーの全ビルドキャッシュの削除に依存しない。
上流版の生成順序が修正されたことを検証できたらローカルpatchを除去できる。

修正後は生成コードの同じlookupが35となり、実放送でPLAYINGと正常終了を確認した。
証跡はGit対象外のbenchmark/viewing-design/guide-watch-recovered.log。
この起動確認は番組表から実際に選局した試験とは区別する。

増分の回帰確認では、新しい出力ディレクトリーのplugin.qmltypesからwatch_program項目を
除いて旧スキーマを再現し、Rustソースのmtimeのみを更新して同じ場所へ再ビルドした。
qmltypesへメソッドが戻り、AOTのlookupも35になったことを検査した後、実放送の
PLAYINGと正常終了を確認した。キャッシュ削除はしていない。生成物の操作はGit対象外の
build内だけで、アプリのRustソース内容は変更していない。
証跡はbenchmark/viewing-design/guide-watch-stale-output.txt、guide-watch-current.qmltypes、
guide-watch-incremental.log。入力元スキーマの改変を製品設定や実行時フォールバックにしない。
