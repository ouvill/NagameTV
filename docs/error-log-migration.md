# 再生エラーログの移植

mainのruntime.rsにあるWritePlaybackErrorとqt_helpers.hの保存先・フォルダー表示を移植。
最新の再生失敗をplayback-error.logへ保存する。通常のステータス更新や再試行で過去の
ディスク上のエラーを消さない。設定画面の「ログフォルダーを開く」はmainの寸法と配色を使う。

Qt側は[QStandardPaths](https://doc.qt.io/qt-6/qstandardpaths.html)のStateLocationを使用。
アプリ名をmainと同じmirakurun-viewerに設定する。Qt 6.7未満ではmainと同じOS別の
保存先判定を維持する。Linuxの標準は~/.local/state/mirakurun-viewer。
取得したパスが空または相対ならRust側がInvalidDirectoryを返し、作業ディレクトリーへ
暗黙に書き込まない。IOエラーは保存先とsourceを持つthiserrorの型で扱う。

ErrorLogは保存処理だけを担当し、Qt・再生・ネットワーク・タイマーを持たない。
再生失敗の通知時に同期で1回書き込む。mainと同様にGUIスレッド上で動くため、
低速な保存先での停止時間は未測定であり、フレーム時間に影響しないとは保証できない。
ディスク上の保持は最新1件、最大64KiB。長い内容はUTF-8境界で切り、省略したことを明記する。
UI上の元のエラー詳細は省略しない。既存tempfile依存を使い、同じディレクトリーの
一意な一時ファイルから置換する。失敗時に一時ファイルを解放し、作成済みの保存先を
書きかけの内容で切り詰めない。電源断に対する永続化保証は持たない。

フォルダーを開く操作は[QDesktopServices::openUrl](https://doc.qt.io/qt-6/qdesktopservices.html#openUrl)
へローカルファイルURLを渡す。返り値trueは外部アプリへの要求成功であり、その後の
ファイルマネージャーの正常表示までは保証しない。保存／起動要求の失敗はlog_errorに
保持して設定画面に表示し、再生そのもののエラーとは別に扱う。

CPUテスト2件で最新内容への置換、UTF-8とサイズ上限、空・相対パス拒否、保存先が
ディレクトリーの場合の失敗と既存内容保持、一時ファイルの後始末を確認した。
アプリ実行時の保存先判定、実再生失敗による保存、ファイルマネージャー起動は未検証。
継続的な資源計測ログ、Qt GCログとそのローテーションは別の未移植項目として残る。

全ターゲットClippy、fmt、SettingsDrawerのqmllint、CMakeリリースビルドも成功。
音声接続問題の解消確認がないため、この変更で実アプリは起動していない。

## ログフォルダー起動失敗のUI確認（2026-09-07）

専用Xvfb :99 / llvmpipeで設定画面の「Open log folder」を操作した。
このコンテナーにはxdg-open・一般的なファイルマネージャーがなく、Qtは
Unable to detect a launcherを記録し、設定画面にも開けなかった旨が表示された。
成功時の外部アプリ表示は未検証であり、この結果を成功経路の確認とはしない。

英語UIでもこの案内が日本語固定だったため、Rustは英語の翻訳ソースを返し、
QML表示境界でqsTranslateへ通す構成へ変更した。保存エラー等の診断文は対応する
翻訳がなければ元の文字列を表示する。追加の通信・保持データはない。
修正版の実アプリで英語の失敗表示を確認し、そのエラーを保持したまま日本語へ変更すると
日本語の案内へ更新された。アプリは終了コード0。

191項目の翻訳テスト・カタログ欠落テスト、QML全84件、fmt・diff検査、
CMakeリリースビルド成功。証跡はbenchmark/virtual-ui/log-folder*.logと
log-folder-failed.png・log-folder-en.png・log-folder-ja.png。
実アプリからの再生エラー保存は別途503-guidanceの363-byteログで確認済み
（feature-migration.md参照）。本節はデスクトップ連携の失敗経路の検証。

## エラー詳細のクリップボード連携（2026-09-07）

mainの`qml/Main.qml`も読み取り専用・マウス選択可能なTextAreaを用いることを照合。
製品コードc910bbbを専用Xvfb :99 / llvmpipeで起動し、ローカル中継で配信要求に503を
返した。自動再生による失敗から詳細を開き、本文をクリックしてCtrl+A、Ctrl+Cを操作。
同じ表示サーバー上の別Qtプロセスで`QClipboard::Clipboard`を読み出し、362バイトが
保存されたplayback-error.logの本文（末尾のファイル用改行を除く）と完全一致した。
[QtのX11クリップボード仕様](https://doc.qt.io/qt-6/qclipboard.html#notes-for-x11-users)
にあるマウス選択用Selectionとは別のClipboardを検証した。

選択状態で文字キーを入力してから再度全選択・コピーしても同じ本文であり、
読み取り専用を確認。詳細を開いたまま1440×900から900×560へ縮小し、本文と
閉じるボタンの表示も確認した。Escapeで詳細を閉じ、専用アプリは終了コード0。
ローカル中継プロセスも検証終了後に停止した。通常表示サーバー:0は操作していない。

証跡はgit管理外の`benchmark/clipboard-ui/`内の`player.log`、`proxy.log`、
`copied.txt`、`copied-after-input.txt`、`selected.png`、`details-small.png`、
`read-clipboard.cpp`と`state/mirakurun-viewer/playback-error.log`。
製品コードの変更なし。X11でアプリ稼働中のコピーの確認であり、アプリ終了後の
クリップボード保持やWayland環境での連携は確認していない。
