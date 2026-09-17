# 開発・ビルド・診断

コマンドは、特記がない限りリポジトリーのルートで実行します。
アプリの導入と基本操作は[README](../README.md)を参照してください。
実装とレビューでは[コード規約](coding-conventions.md)に従い、enumとTypestateで
状態・前提条件・操作順序を表す設計を優先します。

## Canonical Workshopの開発環境

[`.workshop/dev.yaml`](../.workshop/dev.yaml)はUbuntu 26.04を使用します。
RustはWorkshopのRust SDK、ネイティブ版・AppImage・Flatpakに必要なUbuntuパッケージは
[プロジェクトSDKのsetup-base](../.workshop/mirakurun-viewer/hooks/setup-base)で導入します。
依存パッケージを追加するときはこの一覧と[check-health](../.workshop/mirakurun-viewer/hooks/check-health)を更新します。
check-healthはコマンド・開発ライブラリー・QML・GStreamerプラグインファイルの存在を確認し、
機器へアクセスせずに実行できます。画面表示・再生や機器の動作確認は別途行います。

GUIテストではGPUだけをホストから共有し、画面と音声はWorkshop内の専用セッションを使います。
ホストのデスクトップ・音声セッションは共有しません。
[専用環境の構成と検証手順](gui-test-environment.md)を参照してください。

SDK定義の変更を既存のWorkshopへ反映するには、**ホスト側**のプロジェクトディレクトリーで実行します。

```sh
workshop refresh
```

起動済みコンテナー内で手動導入したパッケージだけに依存しないようにします。
再構築時のフック実行については[WorkshopのSDK仕様](https://documentation.ubuntu.com/canonical-workshop/stable/reference/sdks/)を参照してください。

## Flatpakパッケージを作る

[Flatpakのビルド手順](flatpak.md#パッケージを作る)を参照してください。
必要なホスト側ツール、SDK、依存関係の更新方法、出力先を記載しています。

## AppImageパッケージを作る

ネイティブ版の開発環境で`./scripts/build-appimage.sh`を実行します。
Linux x86_64向けのAppImageとSHA-256を`build/appimage/`へ出力します。
必要な追加ツールとOSの互換性条件は[AppImageのビルド手順](appimage.md)を参照してください。

## ネイティブ版をビルドする

ホストに次の開発環境と実行用プラグインが必要です。

- Rust / Cargo（Rust 1.98.1でビルド確認）
- CMake 3.24以降、C/C++コンパイラー、pkg-config、libclang
- Qt 6.8以降のQuick / Controls / Dialogs / Layouts / Shapes / EffectsとQtCore QMLモジュール、LinuxではQt DBus、SVG・JPEG・WebP画像プラグイン、翻訳用の`lrelease`
- GStreamer 1.24以降と開発ライブラリー（`gstreamer-mpegts-1.0`を含む）
- GStreamerの`qml6glsink`、OpenGL関連プラグイン、`tsdemux`、映像・音声デコーダー、音声出力プラグイン

Ubuntuでは`lrelease`は`qt6-l10n-tools`、MPEG-TSの開発ライブラリーは`libgstreamer-plugins-bad1.0-dev`に含まれます。
WebP画像の保存には`qt6-image-formats-plugins`が必要です。
日本語UIのフォントにはNoto Sans CJK JPを使用します。字幕用ARIBフォントは同梱しています。
Linuxのファイル・フォルダー選択はPortalを優先します。ネイティブ版で利用するには
QtのPortalプラグイン（Ubuntuでは`qt6-xdgdesktopportal-platformtheme`）と、
デスクトップ側の`xdg-desktop-portal`および対応バックエンドが必要です。
起動時に利用できない場合はQt Quick製のダイアログを使います。[選択方針](platform-startup.md)

リポジトリーのルートで実行します。

```sh
git submodule update --init
cmake -S . -B build
cmake --build build
./build/mirakurun-viewer
```

字幕デコーダーのlibaribcaptionとTS整形のtsreadexはサブモジュールからビルドします。
映像表示にはOpenGL、音声再生には利用可能な音声出力が必要です。
LinuxではX11とWaylandの両方の環境変数がある場合、Qtの表示先が未指定なら互換設定として`xcb`を選びます。
明示した`QT_QPA_PLATFORM`は優先します。[表示環境の扱い](platform-startup.md)

接続先などを起動時に指定する場合:

```sh
MIRAKURUN_SERVER=http://192.168.1.100:40772 MIRAKURUN_AUTOPLAY=1 ./build/mirakurun-viewer
```

`MIRAKURUN_SERVICE_ID`で選択局を上書きできます。
`MIRAKURUN_AUTOPLAY`は未指定なら保存済みの自動再生設定（初期値OFF）を使います。
`0`で無効、それ以外の指定値で有効になり、この上書きは設定ファイルに保存しません。
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

RustとQMLのプロパティ・通知・起動処理を変更した場合は、次も実行します。

```sh
bash scripts/test-connection.sh
bash scripts/test-startup.sh
```

接続テストは機器を使用せず、Qt通知時の状態の整合性も確認します。起動テストは
専用画面・実GPU・仮想音声を起動して検証した後、製品の`Main.qml`を読み込み、初回・設定済み起動・
番組表の開閉・再生エラー・終了を確認します。設定先は一時ディレクトリーです。
画面部品を変更した場合は、その部品のQMLテストも実行してください。
スクリーンショットの連写・保存・設定変更の試験は`bash scripts/test-screenshot.sh`で実行します。
この試験も専用セッションを自動起動し、画像を一時ディレクトリーに保存して終了時に削除します。
元映像の取得・字幕／コメント合成・連写中の描画は
`bash scripts/test-startup.sh screenshot-playback`で製品の画面を使って検証します。
フレーム番号入りの合成映像をCPUで生成するため、GStreamerの`timeoverlay`と`avenc_mpeg2video`も必要です。
比較画像と計測値は`build/screenshot-review/`へ出力します。

製品のQMLコンポーネントは`rust/qml/`直下に置きます。`rust/build.rs`がこのディレクトリーの
`.qml`ファイルを列挙して登録するため、ファイル一覧の追記は不要です。
`rust/qml/tests/`のテスト用コンポーネントは製品モジュールへ含めません。

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
- [共通TS入力・ライブ振り返り](ts-input-implementation.md)
- [TS録画の再生](recording-playback.md)
- [録画シーク・TS番組情報取得のロードマップ](recording-seek-roadmap.md)
- [遠隔操作API・Protobufとドキュメント生成](remote-control.md)
- [番組表](guide-calendar.md)・[EPGの更新](epg-event-stream.md)
- [弾幕表示](danmaku.md)・[字幕描画](subtitle-rendering.md)・[音声切り替え](audio-selection.md)
- [動画統計](video-statistics.md)・[操作への反応](ui-feedback.md)
- [検証の記録](verification.md)・[実装移行の経緯](feature-migration.md)

開発資料には実装途中の検証記録も含まれます。現在の動作と過去の状態は、各資料の更新日・追記を確認してください。

長い実録画の冒頭停止・遠方シークを製品画面で調べる場合は、
`bash scripts/test-startup.sh recording-probe /path/to/recording.ts` を使います。
表示・GPU・音声の検証後に実行し、通常の起動試験とは別に約28秒の再生と
境界前後・長い録画の80%位置へのシークを確認します。全編の検査ではありません。
