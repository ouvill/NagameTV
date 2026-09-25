# 開発・ビルド・診断

コマンドは、特記がない限りリポジトリーのルートで実行します。
アプリの導入と基本操作は[README](../README.md)を参照してください。
実装とレビューでは[コード規約](coding-conventions.md)の変更に関係する節を参照します。
状態・前提条件・操作順序を表す設計では、enumとTypestateを優先します。

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
- GStreamerの`qml6glsink`、OpenGL関連プラグイン、`tsdemux`・`qtdemux`・`matroskademux`、映像・音声デコーダー、音声出力プラグイン、速度変更用の`scaletempo`（Good Plug-insの`audiofx`）

Ubuntuでは`lrelease`は`qt6-l10n-tools`、MPEG-TSの開発ライブラリーは`libgstreamer-plugins-bad1.0-dev`に含まれます。
Qtモデルの型情報生成には、Qt開発パッケージの`qt6core_metatypes.json`も使用します。
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

### 手動ビルド版の削除

起動中のながめTVをすべて終了してから、`build/nagametv`または手動で配置した実行ファイルを削除します。
Linuxでは、実行ファイルを削除しても動作中のプロセスが終了するとは限りません。

`cmake --install`でインストールした場合は、使用したビルドディレクトリーの
`install_manifest.txt`で対象を確認し、インストールしたファイルを削除してください。
アプリ一覧への登録やログイン時の自動起動を自分で設定した場合は、その登録も解除してください。

### ネイティブ版の保存データの削除

アプリを削除しても、ユーザーの設定とキャッシュは残ります。
これらも不要になった場合は、アプリを終了してから次のフォルダーを削除できます。
同じユーザーの手動ビルド版・deb版・AppImage版で共有しています。

| データ | 既定の場所 | 保存先を変更している場合 |
| --- | --- | --- |
| 設定 | `~/.config/nagametv/` | `$XDG_CONFIG_HOME/nagametv/` |
| キャッシュ | `~/.cache/nagametv/` | `$XDG_CACHE_HOME/nagametv/` |

別の場所へ保存したスクリーンショットや手元の録画ファイルは、必要に応じて保存先で整理してください。
Flatpak版は[専用の削除手順](flatpak.md#アンインストール)を参照してください。

## 起動引数

`./build/nagametv --help`で起動オプションを確認できます。`--version`（`-V`）と
`--build-info`は、それぞれバージョンとビルド情報JSONを出力します。これらの情報表示と
引数エラーは画面・GPU・音声・保存設定を使用しません。未知の引数は終了コード2になります。

検証用の`--features=none`または`--features=subtitles,epg,comments`は従来どおり
その実行で許可する機能を指定します。重複しない任意の組み合わせを使えます。
`=`は必須で、空値・重複・`none`との混在を拒否します。表示方式の指定には
`QT_QPA_PLATFORM`を使用してください。Qt固有の未定義の起動引数は受け付けません。
ビルドした実行ファイルは`python3 scripts/check-cli.py build/nagametv`で機器を使わず検証できます。
ヘルプ・ビルド情報・引数エラーがQt初期化前に終了し、保存データを作らないことを確認します。

## コメントDBのSQL検査

コメントキャッシュのSQLite操作はSQLxの検査付きマクロを使います。
`rust/crates/viewer-comments/.sqlx/`の検査情報を同梱しているため、通常ビルドに
`DATABASE_URL`や検証用DBは不要です。ビルド環境の`DATABASE_URL`を参照させたくない場合は
`SQLX_OFFLINE=true`を指定します。

SQLまたはスキーマを変更した場合は、Python標準ライブラリーのSQLiteで一時DBを作り、
クエリをコンパイルして検査情報を更新します。利用中のキャッシュは操作しません。

```sh
python3 scripts/check-comment-sql.py --update
python3 scripts/check-comment-sql.py
python3 scripts/test.py viewer-comments
python3 scripts/flatpak-cargo-sources.py
```

引数なしのSQL検査は、保存済みの検査情報と新規スキーマとの一致を確認します。CIでも実行します。
DBは専用ワーカーが接続を所有し、SQLxのSQLite処理をそこで待機します。プールや追加の
Tokioランタイムは作りません。行の先読みを1件に制限し、コメントの読み出しは既存の
件数・バイト上限に従ってストリーム処理します。旧DBの移行、書き込み失敗時のロールバック、
WALでの読み書きの並行実行は機器不要のテストで確認します。

一般動画の字幕描画にはlibass 0.17以降の開発ライブラリー（Ubuntu: `libass-dev`、Fedora: `libass-devel`）が必要です。

## テストと診断

変更した機能に対応する試験と、[コード規約の境界ごとの検証](coding-conventions.md#レビューと検証)を
選びます。以下のコマンドは用途別の一覧です。文書の文章だけを変更した場合は、
記述・リンク・差分を確認します。ビルド設定や実行コマンドを変更した場合は、その動作も検証します。
必要な検査が通った後は、追加変更・失敗・未解決の懸念がなければ検査を繰り返しません。

GitHub Actionsでの自動テストと配布ビルド、`main`へのpushに伴う最新Pre-releaseの更新、
バージョンタグからGitHub Releaseの下書きを作成する手順は
[CIとリリース](ci-release.md)を参照してください。

共通ランナーは`python3 scripts/test.py`です。初回は固定版の[nextest](https://nexte.st/docs/installation/pre-built-binaries/)を導入します。
WorkshopとFedoraでは次のコマンドを実行し、`$HOME/.local/bin`を`PATH`へ含めます。
Workshopには同じ導入処理の`setup-tests`アクションがあります。CIイメージには導入済みです。

```sh
bash scripts/install-nextest.sh
python3 scripts/test.py --list
python3 scripts/test.py             # 機器不要: 静的検査、Rust全6パッケージ、Qt接続など
python3 scripts/test.py app         # アプリのRustテストのみ
python3 scripts/test.py danmaku ui-style  # GPU環境を検証してQML部品を確認
python3 scripts/test.py app --filter 'test(settings::)'
```

Rustの通常テストはnextestが個別プロセスで実行します。同じプロセス内のtracing subscriberや
初期化状態の干渉を避け、既定の同時実行数は2、自動リトライは0とします。実時間の再生試験は
nextest内で単独実行します。`--test-threads N`で並列数、`--profile dev`でビルド構成を変更できます。
アプリのpath依存5クレートも明示的に列挙し、nextestの対象外であるdoctestはCargoで別途実行します。
`#[ignore]`の実機・性能プローブ、実EPGStation、配布物の検査は自動では実行しません。

ランナーは必要なバイナリーを先にビルドします。Rustはnextestのビルド情報、Qtは実行ごとのコピーを
使い、各Qtスクリプトで`cargo run`を繰り返しません。ログと`summary.json`は`build/test-runs/run-*/`に
保存します。失敗時はそこで停止し、成功・失敗・環境不足・未実行を区別します。
GUIを選ぶと必要資源を先に検証し、不足時はGUIのビルド・実行へ進みません。

CMakeのビルド、共通ランナー、既存のQtテスト入口は、同じLinuxユーザーの
`/tmp/nagametv-build-UID/lock`を共有します。別worktreeからの起動も待機するため、
ビルドと検証が重なりません。Cargoを直接使う診断やClippyも次のラッパーを通します。
外部のビルドや、ラッパーを使わないコマンドによる負荷までは制御できません。
別コンテナーのビルドも、このロックの対象外です。

```sh
bash scripts/with-build-lock.sh cargo clippy --manifest-path rust/Cargo.toml --release --locked --all-targets -- -D warnings
```

通常は`CARGO_TARGET_DIR=build/cargo`、ビルド並列数2を使います。既存の環境変数で変更できます。
`NAGAMETV_BUILD_LOCK_DIR`は独立したCI環境などでロックの保存先を変えるための設定です。
同時に動く開発作業では保存先を統一してください。

チャンネル選択の操作列は`proptest`、HTTP応答と要求回数は`wiremock`で検証します。
どちらもテスト用の依存です。`proptest`が失敗時に保存した再現用シードは、
修正後も回帰試験に使うためリポジトリーへ含めます。

EPGStationの実装との互換性は、Dockerで固定版の本体を動かす機器不要の試験で検証します。
通常のRustテストとは別に実行します。準備・検証範囲・実応答の更新方法は
[EPGStationの結合テスト](epgstation.md#固定版の実サーバーとの結合テスト)を参照してください。

```sh
CARGO_TARGET_DIR=build/cargo python3 scripts/epgstation-integration.py
```

EPGイベント接続の停止・再試行は、機器不要の独立したクレートでも検証します。
Tokioの仮想時間を使う試験では、実時間の待機を省いて期限前後の動作を確認します。

```sh
python3 scripts/test.py viewer-epg-events
```

依存を変更した場合は`python3 scripts/flatpak-cargo-sources.py`で配布用のソース一覧を更新し、
`python3 scripts/flatpak-cargo-sources.py --check`でロックファイルとの一致を確認します。

Clippyは通常構成とQt統合テスト構成の両方で、警告をエラーとして検査します。

```sh
CARGO_TARGET_DIR=build/cargo SQLX_OFFLINE=true bash scripts/with-build-lock.sh cargo clippy --manifest-path rust/Cargo.toml --release --locked --all-targets -- -D warnings
CARGO_TARGET_DIR=build/cargo SQLX_OFFLINE=true bash scripts/with-build-lock.sh cargo clippy --manifest-path rust/Cargo.toml --release --locked --all-targets --features native_tests -- -D warnings
```

これらのコマンドは表示・GPU・音声機器を使用しません。
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
接続テストは機器を使用せず、Qt通知時の状態の整合性と、`QAbstractItemModelTester`による
チャンネルモデルの更新・絞り込み・元モデルの破棄を確認します。起動テストは
専用画面・実GPU・起動済みPipeWire上の仮想出力を検証した後、製品の`Main.qml`を読み込み、初回・設定済み起動・
番組表の開閉・再生エラー・終了を確認します。設定先は一時ディレクトリーです。
画面部品を変更した場合は、その部品のQMLテストも実行してください。
`bash scripts/test-danmaku.sh`は`rust/qml/tests/`の部品テストを実行します。
共通テーマや部品を変更した場合は、次の検査と部品一覧も実行します。

```sh
python3 scripts/check-ui-style.py
python3 scripts/test-ui-style.py
bash scripts/test-ui-style.sh
```

Pythonの2つの検査は機器不要で、CIにも含めます。部品一覧は専用画面・実GPU・仮想音声出力を
自動検証してから起動し、通常・押下・選択・無効の各状態、ホバー、キーボード操作と
選択欄の開閉を確認します。画像は別途`bash scripts/capture-ui-style.sh`で生成します。
1280×720、640×360、フォーカス・ホバー・選択欄の画像を
`build/ui-review/controls-*.png`へ出力します。自動比較は行わず、ファイルはGit対象外です。
主要画面は`bash scripts/test-startup.sh`でも確認し、画像を`build/navigation-review/`へ保存します。
比較する際は前回の画像を別のディレクトリーへ退避し、同じサイズ・言語・表示内容で見比べます。

チャンネル一覧のホイール操作は`bash scripts/test-channel-wheel.sh`で検証します。
どちらも専用GUI環境を検証し、製品のRust製モデルを登録してから実行します。
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
`pragma Singleton`を持つファイルはsingletonとして登録します。`Theme.qml`などの共有値は
`import MinimalViewer`で参照し、画面のローカルな状態をそこへ保存しないでください。
例外として`CommentList.qml`は評価用featureでのみ登録し、通常版のリソースには含めません。
`rust/qml/tests/`のテスト用コンポーネントは製品モジュールへ含めません。
部品テストも`import MinimalViewer`で製品モジュールを読み込みます。`import ".."`で同じ部品を
別の型として読み込むと、required propertyへ渡すQML型が一致しなくなるため使用しません。
通常の製品モジュールに含まれない評価用`CommentList`だけは、テスト内で`Evaluation`という
別名を付けてソースから読み込みます。
QMLの変更後はテスト用バイナリーを再ビルドしてください。公開テストスクリプトは自動でビルドします。

QMLの静的検査は、ビルド後に次のコマンドで実行します。ビルドと検査には同じ
`CARGO_TARGET_DIR`とQtを指定してください。`QMAKE`を指定したビルドでは、検査にも
同じ値を渡します。省略時は`qmake6`でQtのツールを選びます。

```sh
CARGO_TARGET_DIR=build/cargo bash scripts/check-qml.sh
```

`check-qml.sh`は`rust/qml/`直下の全QMLを列挙し、評価用部品も含めて
`qmllint --max-warnings 0`で検査します。テスト用QMLはこの静的検査の対象外です。
変更ファイルだけに絞らず、警告が1件でもあれば失敗します。CIでもRustビルド後に実行します。
これは機器不要の検査で、GUIの動作テストとは別に結果を確認します。

CXX-Qtが生成する`qml_modules`の場所はCargoから取得します。GStreamerの映像部品は
実行時に型を登録するため、静的検査には[`tools/qmltypes`](../tools/qmltypes/)の型記述も渡します。
この型記述は検査専用で、アプリの読み込み先や配布物には加えません。GStreamer更新時は
上流の公開QML APIとの一致も確認してください。検査設定による警告の格下げや既存警告の
許容リストは設けません。

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
