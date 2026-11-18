# 起動時のQt表示方式

## ファイル選択ダイアログ

Linux版では、録画ファイルとキャプチャ保存先の選択にデスクトップのPortalを優先する。
ネイティブ版とSnapでは、Qtを生成する前にRustからセッションD-Busへ接続し、
`Settings.ReadAll`と`FileChooser`のバージョンを並行して問い合わせる。
接続と2件の問い合わせに共通の1秒の期限を設け、ファイル選択の判定にも同じ結果を使う。

Portalが応答すれば`QT_QPA_PLATFORMTHEME=xdgdesktopportal`を指定し、外観をデスクトップの
テーマに委ねる。QtのPortalプラグインが読み込まれ、フォルダー選択が可能な
`FileChooser`バージョン3以上を確認できれば、Portalのファイル選択を使う。
ファイル選択だけが未対応なら、`AA_DontUseNativeDialogs`でQt Quick製ダイアログを選ぶ。
これによりGTK3への自動フォールバックを避け、X11・HiDPI環境での
`infinite surface size not supported`を回避する。

Portalに接続できない場合や応答が期限に間に合わない場合は、原因をWARNで一度記録する。
`QT_QPA_PLATFORMTHEME=generic`を指定し、Qtによる起動時の同期`Settings.ReadAll`を避ける。
Qt Networkのシステムプロキシ検出を無効にし、GIOは`GIO_USE_PROXY_RESOLVER=dummy`で
直接接続を使う。これでlibproxyによるPortalの再起動待ちも避けられる。
いずれもアプリのプロセスにだけ適用する。表示倍率やX11/Waylandの描画バックエンドには影響しない。

Flatpakの `/.flatpak-info` が存在する場合は、QtのPortal連携を維持する。
サンドボックス外のファイルを選ぶにはホストのPortalとDocument Portalが必要なため、
ネイティブ版用のQt Quick強制は適用しない。Linux以外のダイアログ選択は変更しない。

ネイティブ版のPortal利用にはQtの `xdgdesktopportal` プラグイン、
`xdg-desktop-portal` とデスクトップに対応するバックエンドが必要。
Ubuntuではプラグインは `qt6-xdgdesktopportal-platformtheme` パッケージに含まれる。
Portalの可用性は起動時に決定するため、導入・起動後はアプリも再起動する。

[FileChooserの仕様](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.FileChooser.html)、
[Qt 6.10.2のテーマ選択](https://github.com/qt/qtbase/blob/v6.10.2/src/gui/kernel/qguiapplication.cpp#L1354-L1387)、
[QtのPortal実装](https://github.com/qt/qtbase/blob/v6.10.2/src/plugins/platformthemes/xdgdesktopportal/qxdgdesktopportaltheme.cpp#L107-L138)、
[GIOのプロキシ選択](https://github.com/GNOME/glib/blob/2.88.0/gio/gproxyresolver.c#L78-L89)、
[Qt Networkのシステムプロキシ設定](https://doc.qt.io/qt-6/qnetworkproxyfactory.html#setUseSystemConfiguration)。

2026-09-16: Qt 6.10.2 / GTK 3.24.52 / X11の表示倍率2倍で、最小の
FileDialogでも同じ警告を再現。Wayland経由またはGTKの倍率1倍では再現しなかった。

## 映像表示の選択

ネイティブ版ではQtに表示方式の自動選択を任せる。WaylandとX11の表示先が両方あっても、
アプリから`xcb`を自動指定しない。`QT_QPA_PLATFORM=wayland`または`xcb`を明示できる。
明示設定は空文字・非Unicodeも含めて保持する。

VA-API経路だけはDMA_DRMのEGL importにWaylandが必要なため、未指定の
`QT_QPA_PLATFORM=wayland`と`GST_GL_PLATFORM=egl`をQt起動前に設定する。
必要な環境を利用できなければエラーにする。[GPU映像処理](gpu-video.md)

環境変数の設定は表示サーバーが応答する保証ではない。
試験時には使う表示先を別途検出・検証し、欠けていれば起動しない。

[QtのQGuiApplication仕様](https://doc.qt.io/qt-6/qguiapplication.html#QGuiApplication)
ではQT_QPA_PLATFORMがプラットフォーム選択に使われるため、Qt生成より前に適用する。
[Rustのset_var安全条件](https://doc.rust-lang.org/std/env/fn.set_var.html#safety)
に従い、mainの最初の処理としてQt・GStreamer・診断・ワーカーの開始前に呼ぶ。
環境変更関数をunsafeとして呼び出し側の前提を文書化し、呼び出し位置にも安全性の
理由を記した。

`NAGAMETV_TEST_QPA=wayland python3 scripts/test.py startup -- video-processing`で製品画面の映像処理を検証する。
`NAGAMETV_TEST_QPA=auto`ではQtの自動選択を使う。どちらも
[専用GUI環境](gui-test-environment.md)を起動し、実GPUと仮想音声を検証する。
専用画面はswayを使い、通常の起動・入力試験は既定のX11（rootful Xwayland＋Openbox）で行う。
native Waylandのサイズ変更と全画面切り替えは`startup -- screenshot-playback`で確認済みだが、
起動・入力試験全体はsway上では未確認である。

## 変更の経緯

以前はUbuntu 26.04 / NVIDIA / native WaylandのGL共有で映像が崩れる問題への
暫定対策として、WaylandとX11の両方がある場合に`xcb`を自動指定していた。
関連報告は[GStreamer #5178](https://gitlab.freedesktop.org/gstreamer/gstreamer/-/work_items/5178)。
2026-09-07の検証はこの選択規則とX11の起動が対象で、Wayland描画の試験ではなかった。

2026-09-20: GPU経路・NV12対応後、ユーザー環境でnative Waylandの正常再生の報告があった。
この報告を踏まえ、ネイティブ版の`xcb`自動指定と、削除した選択規則だけを検証するテストを除去した。
QtのGL display/contextを再生前にplaybinへ明示的に渡す修正が改善理由の候補だが、
NV12対応なども同時に変更しており、原因を切り分けた比較試験は行っていない。
この報告は上流の関連問題がすべて修正されたことを証明するものではない。

同日の専用Weston／RTX 4070 Ti／GStreamer 1.28.2で、Qtの自動選択による
`libqwayland.so`の読み込み、yadif＋NV12、NVDEC＋OpenGL＋NV12の再生・映像切り替え・
キャプチャを確認した。Wayland明示指定で字幕・コメントを含むキャプチャ試験も通過した。
起動試験全体はkiosk-shellのサイズ制約で失敗し、試験用desktop-shellでサイズチェックを
通した後もウィンドウのアクティブ化待ちで失敗した。試験の判定は変更していない。
専用環境のWayland検証は映像関連までとし、通常の起動・入力試験はX11で行う。

AppImage／Flatpakは同梱qml6glsinkのWayland対応を別途検証する必要があるため、
パッケージ起動設定の`xcb`は維持する。

## WaylandのIME診断

IMEが単発キーのショートカットを取り込む場合は、問題が起きるデスクトップで
`bash scripts/dev/diagnose-ime.sh`を手動実行する。Waylandの文字入力経路を明示し、
Qtの入力方式選択・フォーカス・文字入力プロトコルのdebugログを
`build/ime-wayland-XXXXXX.log`へ保存する。`bash scripts/dev/diagnose-ime.sh ibus`では
表示方式をWaylandに保ったまま、IBusへの直接接続を指定する。
`QT_IM_MODULES`は`QT_IM_MODULE`より優先されるため、両方を指定して比較する。
環境変数はその起動だけに適用し、デスクトップやIMEの設定ファイルは変更しない。
debugログには入力した文字が含まれることがあるため、再現にはショートカットと
公開してよい試験文字列だけを使う。

これは実際のデスクトップでの手動診断用であり、自動GUIテストからホストの画面へ
接続する用途には使わない。`using input method:`の行で実際の選択を確認する。
`QComposeInputContext`の場合は日本語IMEへ接続できておらず、日本語入力の検証は
できない。専用swayで画面が表示されることだけでは、GNOME等のIME動作の証明にならない。
