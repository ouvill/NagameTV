# Mirakurun Viewer

Qt 6/QML のUIとRustバックエンドを組み合わせたMirakurunライブ視聴クライアントです。
Mirakurunの `/api/services/{id}/stream` をGStreamerで直接再生します。

## アーキテクチャ

- CXX-QtがRustの`Player`をQtの`QObject`として生成し、QMLへ直接公開
- アプリのエントリーポイント、状態管理、再生制御はRustで実装
- 手書きC++は`QQuickItem*`のアドレス取得とQt QuickのOpenGL指定だけを行う
- QMLは表示とユーザー操作に限定し、バックエンド機能を保持しない
- CMakeはWorkshop互換のためCargoビルドを呼び出す薄いラッパーとしてのみ使用
- 通信は専用の単一worker Tokio runtimeで実行し、QtのGUIスレッドをブロックしない
- Mirakurun APIは再利用可能な`reqwest::Client`で取得し、CXX-QtのキューでQtへ返す
- 同じruntimeへNX-JikkyoのWebSocketタスクを追加できる構造にする
- EPGはMirakurunを正本とし、時間順に索引した不変のメモリースナップショットで保持
- EPG更新は完成した新スナップショットとの交換で行い、古いデータを蓄積しない

## 設計上のメモリー境界

- Rustの公式`gstreamer-rs` bindingで`playbin3`の所有権と状態を管理
- 停止・終了時にパイプラインを`NULL`へ戻し、バッファーを解放
- Busイベントは50msごとに空になるまで処理し、アプリ側に蓄積しない
- `qml6glsink`でQt QuickへGLテクスチャを渡し、映像フレームをCPUコピーしない
- YADIFの全フィールド出力でインターレース映像の時間解像度を維持
- 描画待ちqueueを8フレームに制限し、フレームを捨てずにbackpressureをかける
- チャンネル再読み込みは同じ`playbin3`を`READY`へ戻してURIを交換する

これによりアプリ起因の無制限なメモリー増加を防ぎます。YADIF版は実機で
ウォームアップ後のRSS/PSS、FD数、スレッド数が安定することを確認しています。

## ビルド

### Canonical Workshop

必要なツールとライブラリはin-project SDK
`.workshop/mirakurun-viewer/` に定義されています。環境を作り直す場合は
ホスト側から次を実行します。

```bash
workshop refresh dev
workshop run dev build
workshop run dev run
workshop run dev test-ui
workshop run dev test-stream-isolated
```

`project-mirakurun-viewer` SDKのhealth checkは、Rust、CMake、Qt 6、GStreamer、libmpvを
refreshのたびに検証します。GUI、GPU、PulseAudioは既存の`project-gui` SDKと
`.workshop/dev.yaml`の接続定義から提供されます。

`test-ui`はWorkshop内に専用のXvfbディスプレイ`:99`とOpenboxを作り、映像を
自動再生せず軽量にUIを検証します。`test-stream-isolated`は同じ隔離環境でMesaの
ソフトウェアOpenGLとGStreamerのテスト用音声sinkを使い、NHK大津を再生します。
どちらもホストのデスクトップ、入力、音声出力を使用しないため、
`DISPLAY=:99 xdotool ...`で決定論的に操作できます。実GPU、実音声、画質、負荷の
確認には通常の`run`を使用してください。

隔離ディスプレイの解像度も変更できます。

```bash
MIRAKURUN_TEST_SCREEN=1920x1080x24 workshop run dev test-ui
```

アプリのウィンドウはボーダーレスのまま、四辺または四隅のドラッグでサイズを
変更できます。最小サイズは820×480です。

### Ubuntuへ直接導入する場合

Ubuntu 24.04:

```bash
sudo apt install build-essential cmake ninja-build rustc cargo lld \
  fonts-noto-cjk \
  qt6-base-dev qt6-declarative-dev qml6-module-qtquick \
  qml6-module-qtquick-controls qml6-module-qtquick-layouts \
  qml6-module-qtquick-templates \
  libmpv-dev qt6-wayland mpv gstreamer1.0-tools gstreamer1.0-qt6 \
  gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
  gstreamer1.0-plugins-bad gstreamer1.0-libav gstreamer1.0-gl gstreamer1.0-x \
  libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev libaribb24-dev
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build build
```

実行:

```bash
./build/mirakurun-viewer
```

> [!WARNING]
> Ubuntu 26.04のNVIDIA環境では、Qt/GStreamerのネイティブWayland GL共有により
> 映像が緑色に崩れる問題があります。WaylandセッションでXwaylandが利用できる場合、
> アプリは暫定的に`xcb`を選択します。これは恒久的な描画方式ではありません。
> ネイティブWaylandを再検証する場合は、`QT_QPA_PLATFORM=wayland`を明示してください。
> 関連する上流問題は[GStreamer Issue #5178](https://gitlab.freedesktop.org/gstreamer/gstreamer/-/work_items/5178)
> で追跡されています。

```bash
QT_QPA_PLATFORM=wayland ./build/mirakurun-viewer
```

診断や自動試験では環境変数で接続先を指定できます。

```bash
MIRAKURUN_SERVER=http://192.168.3.3:40772 \
MIRAKURUN_SERVICE_ID=3203246080 \
MIRAKURUN_AUTOPLAY=1 \
QT_QPA_PLATFORM=xcb ./build/mirakurun-viewer
```

インターレース解除は`MIRAKURUN_DEINTERLACE`で選択できます。既定は高品質な
`yadif`です。CPU負荷を抑える場合は`linear`、無効化する場合は`off`を指定します。

## 再生エラーログ

再生エラーはUIと端末に表示し、ユーザー別の状態保存ディレクトリに
`playback-error.log`として最新の1件を保存します。端末には保存先の絶対パスも表示します。
設定画面の「ログフォルダーを開く」からOSのファイルマネージャーで確認できます。

Qt 6.7以降は`QStandardPaths::StateLocation`を使用します。
Linuxでは通常`~/.local/state/mirakurun-viewer`（`XDG_STATE_HOME`指定時はその配下）、
Windowsでは通常`%LOCALAPPDATA%/mirakurun-viewer/State`です。
古いQtではLinuxのXDG規約、その他のOSではQtの`AppLocalDataLocation/State`を使用します。
Workshop内ではコンテナ側に保存されます。保存やフォルダー表示の失敗は端末に警告し、
元の再生エラーはそのまま表示します。

## 操作

- 画面下部の`Channels`または`C`: Mirakurunから取得したチャンネル一覧を開く
- `Page Up` / `Page Down`: 前後のチャンネルへ切り替える
- `F11`: フルスクリーン切り替え

チャンネルを選ぶと、再生プロセスを作り直さず同じGStreamerパイプラインのURIを
交換します。地上波、BS、CS、SKYの順にまとめ、地上波はリモコンキー順に表示します。
HTTP接続には5秒、リクエスト全体には10秒のtimeoutを設け、接続プールも上限付きで
再利用します。

EPGは永続化せず、起動後にMirakurunから再構築します。視聴・録画予約などの
ユーザー固有データを追加する段階で、それらだけを別の永続ストアへ保存します。

接続先、最後に選択したService ID、音量はユーザー設定として次のTOMLへ保存します。

```text
$XDG_CONFIG_HOME/mirakurun-viewer/settings.toml
```

`XDG_CONFIG_HOME`が未設定の場合は`~/.config/mirakurun-viewer/settings.toml`です。
`MIRAKURUN_SERVER`と`MIRAKURUN_SERVICE_ID`を指定した場合は、保存値より環境変数を
優先します。

## 長時間試験

`scripts/monitor-memory.sh <PID> [interval]`でRSS、PSS、FD数、スレッド数をCSVへ記録できます。

NHK大津を同じ条件で比較するには、`scripts/benchmark-player.sh mpv`または
`scripts/benchmark-player.sh gstreamer`を実行します。既定の測定時間は10分で、
結果は開始日時ごとの`benchmark/`ディレクトリに保存されます。
ウォームアップ後のメモリーが時間比例で増えないことを判定してください。
