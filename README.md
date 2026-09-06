# Mirakurun Viewer — Feature Lab（Rust）

Qt Quick + GStreamerの最小版から、字幕とEPGを独立して検証するブランチ。
基準は別worktreeの `minimal/qt-gstreamer` / `9fa758d`。通常版mainもそのまま保持する。

このブランチをmainを置き換える実装へ育てます。
コード品質、整理の順序、置き換え条件は [開発方針](docs/main-replacement.md) に記載しています。

## ビルド

Qt 6.8以降の Quick / Controls、GStreamer 1.24以降（qml6glsink・tsdemux・映像デコーダー・
pulsesink）、Rust、CMake、C++コンパイラー、libclangが必要。
PMT通知の取得にはgstreamer-mpegts-1.0の開発パッケージも必要。Ubuntuでは
`libgstreamer-plugins-bad1.0-dev`を導入する。アプリがリンクする追加ライブラリーはlibgstmpegts。

```sh
git submodule update --init
cmake -S . -B build
cmake --build build
CARGO_TARGET_DIR=build/cargo cargo test --manifest-path rust/Cargo.toml --release --locked
```

ネイティブARIB字幕デコーダーは固定コミットのlibaribcaptionからビルドする。
Rustの自動テストは表示・GPU・音声機器を使わない。GStreamerの実TS demuxはメモリー上で検証する。
音声切り替えのCPU結合試験にはtestsrcbin（GStreamer Bad Plug-insのdebugutilsbad）が必要。
生成音声の出力サンプルと映像の継続を試験用sinkで測定する。Qtの画面試験は実際の表示環境を使う。

## 起動・操作

```sh
MIRAKURUN_SERVER=http://192.168.3.3:40772 QT_QPA_PLATFORM=xcb ./build/mirakurun-viewer
```

通常起動は設定から接続先・選択局・音量・字幕・EPGを復元します。設定がなければ字幕OFF・EPG ONです。
接続後は選択局を復元して待機し、チャンネルを選ぶか再生ボタンで開始します。
チャンネルは放送種別・リモコン番号順に表示し、地デジ／BS／CS／SKYなどで絞り込めます。
種別を変えるだけでは再生局は変わりません。
`MIRAKURUN_AUTOPLAY=1` で取得後に自動再生し、`MIRAKURUN_SERVICE_ID` で選択局を上書きできます。
PgUp/PgDownで選局、Gで番組表、F11で全画面を切り替えます。
Cまたは「チャンネル」でロゴ付きのチャンネル選択画面を開きます。
矢印キーで移動しEnterで選局できます。[仕様と検証](docs/channel-browser.md)。
Escapeはポップアップ、チャンネル選択、番組表、動画統計、全画面の順に閉じます。
文字入力中はC・GとPgUp/PgDownによる画面・選局操作を抑止します。
チェックボックスで機能を切り替えます。[ウィンドウ操作の仕様と検証](docs/window-actions.md)。

操作部は映像の上に重なり、再生中は無操作が3.2秒続くと隠れます。マウス移動で再表示します。
文字入力・番組表・ポップアップ・音量ドラッグ中は表示を維持します。
[表示制御と資源管理](docs/overlay-visibility.md)。

- **字幕**：解析と表示を有効化。再生中の変更は一度ストリームを再接続する。
- **字幕を表示**：OFFでも解析は継続し、表示だけ破棄。解析と描画の比較用。
- **EPG**：番組情報を取得し、5分ごとに更新。再生停止中も利用できる。
- **番組表**：選択局の7日分を日付ごとに表示。番組を押すと詳細を開き、閉じると表示データを破棄する。
- **更新**：EPGを手動更新。実行中の取得に重ねて通信しない。

## 同条件でのメモリー比較

毎回新規プロセスで同じ局・表示サイズ・時間・選局回数に揃える。環境変数は上記と同じ。

```sh
./build/mirakurun-viewer --features=none
./build/mirakurun-viewer --features=subtitles
./build/mirakurun-viewer --features=epg
./build/mirakurun-viewer --features=subtitles,epg
```

指定した機能だけが起動時に有効になり、他の機能はUIからも有効化できない。
標準エラーの `METRICS` 行に10秒間隔でVmRSSと機能の保持件数を記録する。
必要なら `2> benchmark/subtitles.log` のように保存する。

字幕は解析のみ／表示ありを比較。EPGは番組表を閉じた状態／開いた状態を比較する。
短時間の増減だけでリークと判断せず、繰り返しで頭打ちになるか確認する。
停止・機能OFF後は字幕購読と待機数、EPG番組数・通信数がゼロになることを確認する。
EPGの通信取消しが完了するまでは停止待ちになる。

詳しい所有関係、停止順序、上限と通常版との差は [architecture.md](docs/architecture.md)。

実施済みの確認と未検証範囲は [verification.md](docs/verification.md) に記載。

配信中にHTTPの途中再開を拒否された場合は、新規接続で1回復旧します。
繰り返し失敗する場合は停止します。[再現・修正の記録](docs/live-stream-errors.md)。

停止・再開時のRSS増加については [allocator-investigation.md](docs/allocator-investigation.md)
を参照。Linux/glibcでは `ALLOC` 行に使用中・空き領域・直接mmap確保量をKiBで出力します。

Linux/glibcでは起動時に `M_MMAP_THRESHOLD` を128KiBに固定します（環境変数不要）。
EPG有効時の停止・再開で、解放済み領域が大量に残る挙動を抑えるためです。
設定理由とアロケーター変更の選択肢は [allocator-controls.md](docs/allocator-controls.md)。
`HTTP_JSON` 行に受信バッファー、`EPG_MEMORY` 行に解析後の保持容量をバイトで記録します。

型付きエラーとEPG取得状態の整理、および回帰検証は [refactoring-verification.md](docs/refactoring-verification.md)。

mainからの機能移植状況と設定保存の仕様は [feature-migration.md](docs/feature-migration.md)。

`MIRAKURUN_DEINTERLACE=yadif|linear|off` で起動時の映像処理を指定できます。
「動画統計」で入力・出力形式、sinkとキューの集計を表示します。
表示中だけ1秒ごとに更新します。[仕様と検証](docs/video-statistics.md)。

字幕は同梱ARIBフォントと輪郭描画を使用します（Qt 6.6以降）。
描画と資源の検証手順は [docs/subtitle-rendering.md](docs/subtitle-rendering.md) を参照してください。

EPG有効時は選択局の現在番組・放送時間・進行率を表示します。番組名を押すと詳細を開けます。
取得は5分間隔、現在番組と進行率は取得済みデータから1秒間隔で更新します。
