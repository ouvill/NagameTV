# Mirakurun Viewer — Feature Lab（Rust）

Qt Quick + GStreamerの最小版から、字幕とEPGを独立して検証するブランチ。
基準は別worktreeの `minimal/qt-gstreamer` / `9fa758d`。通常版mainもそのまま保持する。

## ビルド

Qt 6 Quick / Controls、GStreamer 1.24以降（qml6glsink・tsdemux・映像デコーダー・
pulsesink）、Rust、CMake、C++コンパイラー、libclangが必要。

```sh
git submodule update --init
cmake -S . -B build
cmake --build build
CARGO_TARGET_DIR=build/cargo cargo test --manifest-path rust/Cargo.toml --release --locked
```

ネイティブARIB字幕デコーダーは固定コミットのlibaribcaptionからビルドする。
自動テストは表示・GPU・音声を使わない。GStreamerの実TS demuxはメモリー上で検証する。

## 起動・操作

```sh
MIRAKURUN_SERVER=http://192.168.3.3:40772 QT_QPA_PLATFORM=xcb ./build/mirakurun-viewer
```

起動時は字幕・EPGとも無効。接続後にチャンネルを選ぶか再生ボタンで開始する。
PgUp/PgDownで選局。チェックボックスで機能を切り替える。

- **字幕**：解析と表示を有効化。再生中の変更は一度ストリームを再接続する。
- **字幕を表示**：OFFでも解析は継続し、表示だけ破棄。解析と描画の比較用。
- **EPG**：番組情報を取得し、5分ごとに更新。再生停止中も利用できる。
- **番組表**：選択局の現在から24時間先の一覧。閉じると表示データを破棄する。
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
