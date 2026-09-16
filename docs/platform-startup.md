# 起動時のQt表示方式

## ファイル選択ダイアログ

Linux版では、録画ファイルとキャプチャ保存先の選択にデスクトップのPortalを優先する。
Qt生成前の `DialogSetup::prepare()` で `QT_QPA_PLATFORMTHEME=xdgdesktopportal` を指定し、
Qt生成後・QML読込前に同じトークンの `finish()` で利用可否を判定する。
プラットフォームテーマの指定はこの方針で上書きするが、表示倍率やX11/Waylandの
描画バックエンドは変更しない。外観はQtのPortalテーマがデスクトップのテーマへ委譲する。

ネイティブ版では、QtのPortalプラグインが実際に読み込まれ、セッションD-Busの
`org.freedesktop.portal.FileChooser` がバージョン3以上を返すことを確認する。
バージョン3はフォルダー選択にも必要。確認用のD-Bus呼び出しは1秒でタイムアウトする。
起動時に利用できなければ理由をログに残し、
[`AA_DontUseNativeDialogs`](https://doc.qt.io/qt-6/qt.html#ApplicationAttribute-enum)
でQt Quick製ダイアログを選ぶ。Qtの自動フォールバックでGTK3へ戻るのを防ぎ、
X11・HiDPI環境での `infinite surface size not supported` を回避する。

Flatpakの `/.flatpak-info` が存在する場合は、QtのPortal連携を維持する。
サンドボックス外のファイルを選ぶにはホストのPortalとDocument Portalが必要なため、
ネイティブ版用のQt Quick強制は適用しない。Linux以外のダイアログ選択は変更しない。

ネイティブ版のPortal利用にはQtの `xdgdesktopportal` プラグイン、
`xdg-desktop-portal` とデスクトップに対応するバックエンドが必要。
Ubuntuではプラグインは `qt6-xdgdesktopportal-platformtheme` パッケージに含まれる。
Portalの可用性は起動時に決定するため、導入・起動後はアプリも再起動する。

[FileChooserの仕様](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.FileChooser.html)、
[QtのPortal実装](https://github.com/qt/qtbase/blob/6.10/src/plugins/platformthemes/xdgdesktopportal/qxdgdesktopportaltheme.cpp)。

2026-09-16: Qt 6.10.2 / GTK 3.24.52 / X11の表示倍率2倍で、最小の
FileDialogでも同じ警告を再現。Wayland経由またはGTKの倍率1倍では再現しなかった。

## 映像表示の互換設定

mainのrust/src/main.rsにあるapply_temporary_xcb_workaroundを移植した。
mainのREADMEでは、Ubuntu 26.04 / NVIDIA / native WaylandのGL共有で映像が崩れる
問題への暫定対策としている。関連リンクは
[GStreamer #5178](https://gitlab.freedesktop.org/gstreamer/gstreamer/-/work_items/5178)。
今回この上流ページは取得できず、現在の修正状況は未確認。対策を不要と判断せず、
mainの選択条件を維持する。

LinuxでQT_QPA_PLATFORMが未指定、WAYLAND_DISPLAYとDISPLAYが両方空でない場合だけ
xcbを指定する。明示設定は空文字・非Unicodeも含めて保持する。
Waylandのみ、X11のみ、表示変数なしの場合は変更しない。
この条件は環境変数による方針選択であり、表示サーバーが応答する保証ではない。
試験時には使う表示先を別途検出・検証し、欠けていれば起動しない。

[QtのQGuiApplication仕様](https://doc.qt.io/qt-6/qguiapplication.html#QGuiApplication)
ではQT_QPA_PLATFORMがプラットフォーム選択に使われるため、Qt生成より前に適用する。
[Rustのset_var安全条件](https://doc.rust-lang.org/std/env/fn.set_var.html#safety)
に従い、mainの最初の処理としてQt・GStreamer・診断・ワーカーの開始前に呼ぶ。
環境変更関数をunsafeとして呼び出し側の前提を文書化し、呼び出し位置にも安全性の
理由を記した。条件判定はPolicy enumを返す純粋関数で、テストは環境を変更しない。
unwrapや新しい常駐処理は追加していない。

明示設定の優先、空文字、非Unicode、Waylandのみ・X11のみ・両方の条件を
デバイスを使わないユニットテストで確認する。native Waylandの描画修復や
GPU性能は、この選択規則の検証だけでは確認できない。

2026-09-07: 対象テスト2件、全ターゲットClippy（警告をエラー扱い）、書式検査、
CMakeリリースビルド成功。専用Xvfb :99はxdpyinfoとglxinfoで事前確認した。
WAYLAND_DISPLAYへ試験専用の値policy-test-onlyを与え、QT_QPA_PLATFORMを未指定にして
起動した。この値は分岐入力の模擬であり、Waylandサーバーへ接続する試験ではない。
実際の表示先は検証済みの:99、音声は明示fakesink、全追加機能無効、再生停止中。
xcb互換モードの起動ログ、プロセスのlibqxcb.soマッピング、表示された待機画面を確認した。
閉じるボタンから終了コード0。QML例外・GStreamer criticalはログになかった。
証跡はGit対象外のbenchmark/platform-startup/にplayer.log、platform-maps.txt、
started.png、専用設定・診断ログとして保存した。実GPUの検証プロセスは維持した。
