# 開発・ビルド・診断

コマンドは、特記がない限りリポジトリーのルートで実行します。
アプリの導入と基本操作は[README](../README.md)を参照してください。

## Flatpakパッケージを作る

[Flatpakのビルド手順](flatpak.md#パッケージを作る)を参照してください。
必要なホスト側ツール、SDK、依存関係の更新方法、出力先を記載しています。

## ネイティブ版をビルドする

ホストに次の開発環境と実行用プラグインが必要です。

- Rust / Cargo（Rust 1.98.1でビルド確認）
- CMake 3.24以降、C/C++コンパイラー、pkg-config、libclang
- Qt 6.8以降のQuick / Controls / Layouts / Shapes、SVG画像プラグイン、翻訳用の`lrelease`
- GStreamer 1.24以降と開発ライブラリー（`gstreamer-mpegts-1.0`を含む）
- GStreamerの`qml6glsink`、OpenGL関連プラグイン、`tsdemux`、映像・音声デコーダー、音声出力プラグイン

Ubuntuでは`lrelease`は`qt6-l10n-tools`、MPEG-TSの開発ライブラリーは`libgstreamer-plugins-bad1.0-dev`に含まれます。
日本語UIのフォントにはNoto Sans CJK JPを使用します。字幕用ARIBフォントは同梱しています。

リポジトリーのルートで実行します。

```sh
git submodule update --init
cmake -S . -B build
cmake --build build
./build/mirakurun-viewer
```

字幕デコーダーのlibaribcaptionはサブモジュールからビルドします。
映像表示にはOpenGL、音声再生には利用可能な音声出力が必要です。
LinuxではX11とWaylandの両方の環境変数がある場合、Qtの表示先が未指定なら互換設定として`xcb`を選びます。
明示した`QT_QPA_PLATFORM`は優先します。[表示環境の扱い](platform-startup.md)

接続先などを起動時に指定する場合:

```sh
MIRAKURUN_SERVER=http://192.168.1.100:40772 MIRAKURUN_AUTOPLAY=1 ./build/mirakurun-viewer
```

`MIRAKURUN_SERVICE_ID`で選択局を上書きできます。
`MIRAKURUN_AUTOPLAY`は未指定または`0`で無効、それ以外の指定値で有効です。
`MIRAKURUN_DEINTERLACE=yadif|linear|off`で起動時の映像処理を指定できます。
音声出力の選択は[音声出力](audio-output.md)を参照してください。

## テストと診断

アプリのRustテスト:

```sh
CARGO_TARGET_DIR=build/cargo cargo test --manifest-path rust/Cargo.toml --release --locked
```

このコマンドは表示・GPU・音声機器を使用しません。
音声切り替えのCPU結合試験にはGStreamer Bad Plug-insの`testsrcbin`が必要です。
Qtの画面試験は別の実行手順で、表示環境などを確認してから起動します。[Qtテスト](qt-tests.md)

通常ログは標準エラーへ出力し、既定は`info`以上です。
`RUST_LOG=debug`で詳細ログ、`RUST_LOG=info,qt=debug`でQt/QMLのdebugログも表示できます。
`METRICS`は字幕・EPGの保持件数などを10秒ごとに出すdebugログです。
RSSなどの資源使用量は診断JSONLに記録します。[メモリー分析と記録の切り替え](memory-profiling.md)

開発用の`--features=none|subtitles|epg|comments`は、起動する機能を制限します。
複数指定は`--features=subtitles,epg`のようにカンマで区切ります。
この起動では通常の設定を読み書きせず、除外した機能はUIからも有効化できません。
診断JSONLも記録する場合は`MIRAKURUN_DIAGNOSTICS=1`を明示します。
字幕の表示／非表示は通常の設定で変更できます。

## 開発資料

- [構成と資源の所有関係](architecture.md)
- [番組表](guide-calendar.md)・[EPGの更新](epg-event-stream.md)
- [弾幕表示](danmaku.md)・[字幕描画](subtitle-rendering.md)・[音声切り替え](audio-selection.md)
- [動画統計](video-statistics.md)・[操作への反応](ui-feedback.md)
- [検証の記録](verification.md)・[実装移行の経緯](feature-migration.md)

開発資料には実装途中の検証記録も含まれます。現在の動作と過去の状態は、各資料の更新日・追記を確認してください。
