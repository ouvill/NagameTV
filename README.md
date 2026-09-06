# Mirakurun Viewer — Minimal Rust

Rust + Qt 6/QML + GStreamerの最小視聴アプリです。
ブランチ: `minimal/qt-gstreamer`。通常版とは独立した比較用実装です。

## 機能

- Mirakurunの `/api/services` でチャンネル一覧を取得
- `/api/services/{id}/stream` の映像・音声を再生
- チャンネル選択、前／次（Page Up / Page Down）、再生・停止、音量
- 接続・再生エラーを画面と端末に表示

字幕、実況、番組表、ロゴ、番組情報、設定保存、自動更新はありません。
チャンネル一覧はサーバーの順序を使用し、type=1のTVサービスを表示します。
初回接続後は再生ボタン、またはチャンネル選択で再生を開始します。

## 構成

| ファイル | 責任 |
| --- | --- |
| `rust/src/main.rs` | QtとGStreamerの初期化 |
| `rust/src/player.rs` | UI操作、状態、取得結果・Busの処理 |
| `rust/src/playback.rs` | 一つのplaybin3と映像・音声sinkの所有 |
| `rust/src/services.rs` | チャンネル取得・検証、容量1件の結果キュー |
| `qml/Main.qml` | 操作と映像表示 |
| `rust/src/qt_helpers.h` | OpenGL指定とQQuickItemポインターの橋渡しのみ |

アプリのロジックはRustです。CXX-QtでQObjectを公開します。
通信は1ワーカーのTokioで処理し、GUIスレッドでHTTP完了を待ちません。
要求の置換は旧タスクと旧受信キューを破棄し、古いサーバーの結果を適用しません。
応答は1 MiB、接続5秒、要求全体10秒に制限します。

映像はvideoconvert → YADIF（全フィールド）→ 8フレームqueue → glupload →
glcolorconvert → RGBA → qml6glsink。音声はpulsesinkを明示的に使います。
選局・停止ではパイプラインをREADYへ戻し、旧配信を停止してからURIを交換します。
NULLへの遷移は終了時だけに限定します。同じ局への再生要求は、接続中・再生中なら何もしません。
通常版と機能・診断負荷が異なるため、メモリー差を単一機能の効果と断定しません。
GStreamerの字幕処理も無効で、アプリ独自のTS解析は行いません。

## ビルド・起動

既存WorkshopのRust、Qt 6、GStreamer SDKを利用できます。
libaribcaption、libmpv、字幕フォントはこのアプリのビルドには使用しません。

```sh
cmake -S . -B build -G Ninja
cmake --build build
QT_QPA_PLATFORM=xcb MIRAKURUN_SERVER=http://192.168.3.3:40772 ./build/mirakurun-viewer
```

サーバーは画面からも入力できます。通常版の保存設定は読み書きしません。
NVIDIA環境の比較では通常版と同じ`QT_QPA_PLATFORM=xcb`を明示してください。
ディスプレイ・OpenGL GPU・PulseAudio接続が必要です。自動でソフトウェア描画や
無音出力へ切り替える処理はありません。

このブランチのworktreeでビルド・起動してください。元の`/project`のバイナリは通常版です。
共有SDKには通常版用の追加ツールも残りますが、このアプリからは呼び出しません。

## 検証・メモリー計測

デバイスを使用しない取得データ・要求寿命のテスト:

```sh
CARGO_TARGET_DIR=build/cargo cargo test --manifest-path rust/Cargo.toml --release --locked
```

実ディスプレイ・GPU・音声の接続を確認した後に、手動で再生と複数回の選局、
停止・再生、接続失敗からの再接続を確認してください。

```sh
scripts/monitor-memory.sh <PID> 10
QT_QPA_PLATFORM=xcb MIRAKURUN_SERVER=http://192.168.3.3:40772 \
  heaptrack --record-only ./build/mirakurun-viewer
```

releaseにもデバッグ行情報を付け、プロファイラーで確保元を追えるようにしています。

### 今回の確認（2026-09-06）

- CMakeのreleaseビルドと3件のRustテストが成功。
- 実ディスプレイ・NVIDIA OpenGL・PulseAudioで起動。
- NHK総合2京都、NHK総合1大津、NHK Eテレ1大阪への切り替えで
  PLAYING到達と映像表示を確認。停止・同じ局の再開も確認。
- 最終ビルドでQtの更新スレッドに関する警告を一度観測。再生は継続したが、
  この警告の発生元と長時間の安定性は未検証。
- 長時間のメモリー安定性を保証する計測はまだ行っていない。

ローカルの画面・ログは`benchmark/verification/`に保存（Git対象外）。

### チャンネル切り替えのクラッシュ修正

旧版では再利用するplaybin3を選局ごとにNULLへ戻していた。
実環境でstreamsynchronizerのpad名重複に続くplaysinkのassertionによる異常終了を観測した。
選局と停止をREADYへの遷移に変更し、同じ局の重複要求も抑止した。
停止中はGL表示基盤を再利用用に保持し、終了時にNULLで解放する。

回帰テストは、実際のGStreamer streamsynchronizerが既存padを保持する状況で、
停止後の新しいpad名が衝突しないことを検証する。ディスプレイ・GPU・音声は使わない。
[上流実装](https://github.com/GStreamer/gstreamer/blob/1.28.2/subprojects/gst-plugins-base/gst/playback/gststreamsynchronizer.c)
でもREADY→NULLで番号をリセットすることを確認できる。

修正版の実画面では同じ局への再生20回、通常の往復選局12回、高速選局100操作、
停止・再生20組を実施し、異常終了・pad重複を観測しなかった。
高速操作はGUIの処理状況により入力がまとめられるため、100回の再生完了を意味しない。
ログと実行スクリプトは`benchmark/switch-fix/`に保存（Git対象外）。
