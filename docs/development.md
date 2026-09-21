# 開発・ビルド・診断

コマンドは、特記がない限りリポジトリーのルートで実行します。
アプリの導入と基本操作は[README](../README.md)を参照してください。
実装とレビューでは[コード規約](coding-conventions.md)に従い、enumとTypestateで
状態・前提条件・操作順序を表す設計を優先します。

## Canonical Workshopの開発環境

[`.workshop/dev.yaml`](../.workshop/dev.yaml)はUbuntu 26.04を使用します。
ネイティブ版のRustはWorkshopのRust SDK、開発・パッケージ用のUbuntuパッケージは
[プロジェクトSDKのsetup-base](../.workshop/nagametv/hooks/setup-base)で導入します。
GitHub CLI（`gh`）もプロジェクトSDKに含まれます。
OpenCodeはCanonical提供の[OpenCode SDK](https://github.com/canonical/opencode-sdk)を
`latest/stable`チャンネルから導入します。
配布用AppImageは別のUbuntu 24.04 Docker環境を使います。
依存パッケージを追加するときはこの一覧と[check-health](../.workshop/nagametv/hooks/check-health)を更新します。
check-healthはコマンド・開発ライブラリー・QML・GStreamerプラグインファイルの存在を確認し、
機器へアクセスせずに実行できます。画面表示・再生や機器の動作確認は別途行います。

WorkshopのGUIテストではGPUだけをホストから共有し、画面は専用セッション、
音声はWorkshop起動時に用意するPipeWire上の実行ごとの仮想出力を使います。
FedoraなどのLinuxで直接実行する場合は既存のローカルPipeWireを利用できます。
テストは音声サーバー本体を起動・停止せず、ホストのデスクトップには接続しません。
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

Dockerが利用できる環境で`./scripts/build-appimage.sh`を実行します。
Ubuntu 24.04専用のビルド環境を使い、Workshopの新しいglibcを同梱ライブラリーへ持ち込みません。
Linux x86_64向けのAppImageとSHA-256を`build/appimage/`へ出力します。
必要な追加ツールとOSの互換性条件は[AppImageのビルド手順](appimage.md)を参照してください。

## Ubuntu debパッケージを作る

`./scripts/build-deb.sh 24.04`または`./scripts/build-deb.sh 26.04`を実行します。
対象OSのDocker環境でビルドし、`build/deb/ubuntu24.04/`または`build/deb/ubuntu26.04/`に
debとSHA-256を出力します。24.04用はQtを同梱し、26.04用はシステムのQtを利用します。
[debの構成・検証手順](deb.md)を参照してください。

## ネイティブ版をビルドする

### Fedoraで依存関係を導入する

Fedora向けの[セットアップスクリプト](../scripts/setup-fedora.sh)で、Rustと開発ツール、
Qt/GStreamer、専用GUIテスト用のツールを導入できます。`sudo`の認証とDNFの
トランザクション確認は端末で行います。

```sh
bash scripts/setup-fedora.sh
```

`--assumeno`を付けると、インストールせずに依存解決の結果を確認できます。
2026-09-20にFedora 44のRust 1.98.1、Qt 6.11.2、GStreamer 1.28.7で
全44パッケージの導入とリリースビルドを確認しました。翻訳ツール`lrelease`は`qt6-linguist`、
`qml6glsink`は`gstreamer1-plugins-good-qt6`に含まれます。

GUIテストは既存のPipeWire／pipewire-pulseaudio／WirePlumberと実GPUを必要とします。
スクリプトは音声サービスの起動・変更やコンテナーの設定変更を行いません。
[専用GUI環境の検証](gui-test-environment.md#fedoraなどのlinuxで直接実行)を済ませてから
画面を使う試験を実行してください。AppImage／Flatpak配布用のツールは各配布手順で
別途準備します。

同環境でRustテスト262件（4件はignored）とQt接続テストが成功しました。
専用GUI環境の検証もAMD Radeon Graphics／Mesa 26.2.2／PipeWire 1.6.8で成功しています。
起動テストでは初回起動、録画の時計リセット・PID変更、タイムシフト再生まで成功しましたが、
設定済み起動後の録画ファイルのドロップが`check_recording`の`drop_file_on_root`で
失敗しました。原因は未確定で、起動テスト全体の成功は未確認です。

### 共通の要件とビルド手順

ホストに次の開発環境と実行用プラグインが必要です。

- Rust / Cargo（Rust 1.98.1でビルド確認）
- CMake 3.24以降、C/C++コンパイラー、pkg-config、libclang
- Qt 6.8以降のQuick / Controls / Dialogs / Layouts / Shapes / EffectsとQtCore QMLモジュール、LinuxではQt DBus、SVG・JPEG・WebP画像プラグイン、翻訳用の`lrelease`
- GStreamer 1.24以降と開発ライブラリー（`gstreamer-mpegts-1.0`を含む）
- GStreamerの`qml6glsink`、OpenGL関連プラグイン、`tsdemux`、映像・音声デコーダー、音声出力プラグイン、速度変更用の`scaletempo`（Good Plug-insの`audiofx`）

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
./build/nagametv
```

字幕デコーダーのlibaribcaptionとTS整形のtsreadexはサブモジュールからビルドします。
映像表示にはOpenGL、音声再生には利用可能な音声出力が必要です。
Linuxの表示方式はQtの自動選択に任せます。`QT_QPA_PLATFORM=wayland`または`xcb`で明示できます。
VA-API経路では未指定の表示方式をWaylandにします。[表示環境の扱い](platform-startup.md)

接続先などを起動時に指定する場合:

```sh
NAGAMETV_SERVER=http://192.168.1.100:40772 NAGAMETV_AUTOPLAY=1 ./build/nagametv
```

`NAGAMETV_SERVICE_ID`で選択局を上書きできます。
`NAGAMETV_AUTOPLAY`は未指定なら保存済みの自動再生設定（初期値OFF）を使います。
`0`で無効、それ以外の指定値で有効になり、この上書きは設定ファイルに保存しません。
`NAGAMETV_DEINTERLACE=yadif|linear|off|gl|va`で起動時の映像処理を指定できます。
`NAGAMETV_VIDEO_FORMAT=auto|nv12|rgba`で表示形式を選択します。
GPU経路の前提条件と方式の違いは[GPU映像処理](gpu-video.md)を参照してください。
`NAGAMETV_PLAYBACK_CLOCK=auto|system`で実機比較用の再生時計を指定できます。
音声出力の選択と時計の比較方法は[音声出力](audio-output.md)を参照してください。

コメント表示の方式、関連特許の調査、通常版とローカル評価版のビルドフラグは
[コメント表示の仕様](comment-display-redesign.md)を参照してください。評価版はインストール・配布用ビルドには使えません。

## テストと診断

GitHub Actionsでの自動テストと配布ビルド、`main`へのpushに伴う最新Pre-releaseの更新、
バージョンタグからGitHub Releaseの下書きを作成する手順は
[CIとリリース](ci-release.md)を参照してください。

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
bash scripts/test-desktop-media.sh
bash scripts/test-startup.sh
```

Linuxのメディア連携テストも機器不要で、専用D-Bus・`python3-dbus`・`python3-gi`を使います。
[MPRIS連携の仕様と検証範囲](desktop-media.md)を参照してください。
接続テストは機器を使用せず、Qt通知時の状態の整合性も確認します。起動テストは
専用画面・実GPU・起動済みPipeWire上の仮想出力を検証した後、製品の`Main.qml`を読み込み、初回・設定済み起動・
番組表の開閉・再生エラー・終了を確認します。設定先は一時ディレクトリーです。
画面部品を変更した場合は、その部品のQMLテストも実行してください。
`NAGAMETV_TEST_QPA=wayland bash scripts/test-startup.sh video-processing`で専用Wayland画面を使います。
`NAGAMETV_TEST_QPA=auto`は表示先の明示指定を外し、Qtの自動選択を検証します。
省略時の試験は`xcb`（VA-API経路だけ`wayland`）です。
Wayland試験のサイズ変更・入力フォーカスの制約は[専用GUI環境](gui-test-environment.md)を参照してください。
スクリーンショットの連写・保存・設定変更の試験は`bash scripts/test-screenshot.sh`で実行します。
この試験も専用セッションを自動起動し、画像を一時ディレクトリーに保存して終了時に削除します。
元映像の取得・字幕／コメント合成・連写中の描画は
`bash scripts/test-startup.sh screenshot-playback`で製品の画面を使って検証します。
フレーム番号入りの合成映像をCPUで生成するため、GStreamerの`timeoverlay`と`avenc_mpeg2video`も必要です。
比較画像と計測値は`build/screenshot-review/`へ出力します。

製品のQMLコンポーネントは`rust/qml/`直下に置きます。`rust/build.rs`がこのディレクトリーの
`.qml`ファイルを列挙して登録するため、ファイル一覧の追記は不要です。
例外として`CommentList.qml`は評価用featureでのみ登録し、通常版のリソースには含めません。
`rust/qml/tests/`のテスト用コンポーネントは製品モジュールへ含めません。

通常ログは標準エラーへ出力し、既定は`info`以上です。
`RUST_LOG=debug`で詳細ログ、`RUST_LOG=info,qt=debug`でQt/QMLのdebugログも表示できます。
`METRICS`は字幕・EPGの保持件数などを10秒ごとに出すdebugログです。
RSSなどの資源使用量は診断JSONLに記録します。[メモリー分析と記録の切り替え](memory-profiling.md)

### ビルド情報を確認する

「設定 → 診断 → ビルド情報」で、実行中のアプリに埋め込まれた情報を確認できます。
文字列は選択・コピーできます。画面を起動せずに取得する場合は、次を使います。

```sh
./build/nagametv --build-info
./build/nagametv --version
```

`--build-info`はJSON、`--version`（`-V`）はバージョン番号を標準出力へ出して終了します。
いずれもQt・GStreamerや設定の初期化前に終了し、表示・GPU・音声機器を使用しません。
情報はビルド時に確定するため、配布先のGitや起動時の環境変数には左右されません。

埋め込む項目は、アプリのバージョン、Gitの完全なコミットIDと変更状態、ビルド日時、
ターゲットトリプル、Cargoプロファイル、Rustコンパイラーのバージョン、有効なCargo機能です。
機能名はCargoの`CARGO_FEATURE_`を除いた表記（例: `DISTRIBUTION`）です。
Gitの変更状態は未追跡ファイルとサブモジュールを含み、無視対象のビルド出力は除きます。
通常起動時のinfoログにも出力し、診断JSONLでは各レコードの`build_info`に記録します。
GC専用ログと間引き履歴にも付けるため、ローテーション後もビルドを識別できます。

ビルド日時はUnix秒で保存し、画面ではUTCで表示します。`SOURCE_DATE_EPOCH`を指定すると
その値を使います。未指定時はビルドスクリプトの実行時刻です。Gitの状態を古いまま残さないため、
Cargoを実行するたびにビルド情報を収集し直し、アプリを再ビルドします。

AppImage・debのDockerビルドとFlatpakのビルドスクリプトは、元の作業ツリーで取得した
Git情報をビルド環境へ渡します。独自のソースアーカイブを使う場合は、
`NAGAMETV_BUILD_SOURCE=<完全なコミットID>:clean`または`:dirty`で指定できます。
`.git`も指定値もない場合は`source.kind`を`unavailable`とし、画面に「取得できません」と表示します。
指定値が不正な場合や、存在するGitリポジトリーの情報取得に失敗した場合はビルドを停止します。
収集処理と配布用メタデータの機器不要テストは`python3 scripts/test-build-info.py`で実行します。

### 起動する機能を制限する

開発用の`--features=none|subtitles|epg|comments`は、起動する機能を制限します。
複数指定は`--features=subtitles,epg`のようにカンマで区切ります。
この起動では通常の設定を読み書きせず、除外した機能はUIからも有効化できません。
診断JSONLも記録する場合は`NAGAMETV_DIAGNOSTICS=1`を明示します。
字幕の表示／非表示は通常の設定で変更できます。

## 開発資料

ドキュメント全体のポータルは **[ドキュメント一覧 (docs/README.md)](README.md)** を参照してください。

### アーキテクチャ・設計仕様
- [構成と資源の所有関係 (アーキテクチャ)](architecture.md)
- [UIデザイン方針](ui-design.md)・[操作への反応](ui-feedback.md)
- [ショートカットとウィンドウ・キー操作](shortcut-actions-design.md)
- [チャンネル選局とブラウザー](channel-browser.md)
- [共通TS入力とタイムシフト再生](ts-input-implementation.md)
- [音声機能と出力制御](audio-output.md)
- [TS録画の再生](recording-playback.md)
- [番組表 (EPG)](guide-calendar.md)・[現在番組情報](current-program.md)
- [弾幕表示の仕様](comment-display-redesign.md)・[弾幕のコアと表示](danmaku.md)
- [コメント特許調査と対応方針](comment-patent-review.md)
- [実況過去ログの取得・保持](comment-archive-redesign.md)
- [遠隔操作API・Protobuf](remote-control.md)
- [字幕描画](subtitle-rendering.md)・[動画統計](video-statistics.md)
- [GPU映像処理](gpu-video.md)・[デスクトップメディア連携](desktop-media.md)

### 開発・検証履歴
- [検証の記録](verification.md)・[実装移行の経緯](feature-migration.md)
- [録画シーク・TS番組情報取得のロードマップ](recording-seek-roadmap.md)


開発資料には実装途中の検証記録も含まれます。現在の動作と過去の状態は、各資料の更新日・追記を確認してください。
改名前の検証記録にある`mirakurun-viewer`・`litv`・`MIRAKURUN_`は当時の名称です。
現在の起動コマンドは`nagametv`、環境変数とCMakeオプションの接頭辞は`NAGAMETV_`です。

長い実録画の冒頭停止・遠方シークを製品画面で調べる場合は、
`bash scripts/test-startup.sh recording-probe /path/to/recording.ts` を使います。
表示・GPU・音声の検証後に実行し、通常の起動試験とは別に約28秒の再生と
境界前後・長い録画の80%位置へのシークを確認します。全編の検査ではありません。

録画実況の補助I/Oだけを機器なしで測る場合:

```sh
NAGAMETV_RECORDING_PROBE=/path/to/recording.ts CARGO_TARGET_DIR=build/cargo \
  cargo test --manifest-path rust/Cargo.toml --release --locked \
  recording_metadata_probe_uses_bounded_io_and_stays_idle -- --ignored --nocapture
```

初期情報の取得、80%位置へのシーク、探索後の追加読取り停止を検証します。
通常再生で読む量と補助読取り量を分け、実ファイルは変更しません。

ライブ番組情報のちらつきを受信データから調べる場合は、次の機器不要の試験を使います。
TSの先頭最大64MiBを受信順に解析し、取得済みの番組情報が映像・音声の表示時刻まで
利用できることを確認します。PCRが映像とは別のPIDにある構成も対象です。
デコードや再生は行わず、入力ファイルも変更しません。

```sh
NAGAMETV_LIVE_METADATA_PROBE=/path/to/captured.ts CARGO_TARGET_DIR=build/cargo \
  cargo test --manifest-path rust/Cargo.toml --release --locked \
  captured_broadcast_covers_received_presentation_timestamps -- --ignored --nocapture
```

旧DBの移行確認には、使用中のDBではなく取得済みのコピーを指定します。
試験はさらに一時ディレクトリーへコピーしてから移行・空応答との統合を行います。

```sh
NAGAMETV_COMMENT_CACHE_PROBE=/path/to/copied/cache.sqlite3 CARGO_TARGET_DIR=build/cargo \
  cargo test --manifest-path rust/crates/viewer-comments/Cargo.toml --release --locked \
  --features network copied_legacy_cache_is_preserved_and_completed_as_one_program \
  -- --ignored --nocapture
```

実況の詳細ログは`RUST_LOG=info,comment_archive=debug,recording_metadata=debug`で有効にできます。
要求の対象・範囲・補完／再確認条件、応答バイト数・出典別件数、表示への投影数、
時計待ち／取得の時点、補助I/Oの量を確認できます。コメント本文・ユーザーIDは出力しません。
