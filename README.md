# Mirakurun Viewer

Qt 6/QML のUIとRustの再生コアを組み合わせたMirakurunライブ視聴クライアントです。
Mirakurunの `/api/services/{id}/stream` をGStreamerで直接再生します。

## 設計上のメモリー境界

- Rustの公式`gstreamer-rs` bindingで`playbin3`の所有権と状態を管理
- 停止・終了時にパイプラインを`NULL`へ戻し、バッファーを解放
- Busイベントは50msごとに空になるまで処理し、アプリ側に蓄積しない
- `qml6glsink`でQt QuickへGLテクスチャを渡し、映像フレームをCPUコピーしない
- チャンネル再読み込みは同じ`playbin3`を`READY`へ戻してURIを交換する

これによりアプリ起因の無制限なメモリー増加を防ぎます。実機での長時間RSS/PSS試験は別途必要です。

## ビルド

### Canonical Workshop

必要なツールとライブラリはin-project SDK
`.workshop/mirakurun-viewer/` に定義されています。環境を作り直す場合は
ホスト側から次を実行します。

```bash
workshop refresh dev
workshop run dev build
workshop run dev run
```

`project-mirakurun-viewer` SDKのhealth checkは、Rust、CMake、Qt 6、GStreamer、libmpvを
refreshのたびに検証します。GUI、GPU、PulseAudioは既存の`project-gui` SDKと
`.workshop/dev.yaml`の接続定義から提供されます。

### Ubuntuへ直接導入する場合

Ubuntu 24.04:

```bash
sudo apt install build-essential cmake ninja-build rustc cargo \
  qt6-base-dev qt6-declarative-dev qml6-module-qtquick \
  qml6-module-qtquick-controls qml6-module-qtquick-layouts \
  qml6-module-qtquick-templates \
  libmpv-dev qt6-wayland mpv gstreamer1.0-tools gstreamer1.0-qt6 \
  gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
  gstreamer1.0-plugins-bad gstreamer1.0-libav gstreamer1.0-gl gstreamer1.0-x \
  libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build build
```

実行:

```bash
./build/mirakurun-viewer
```

診断や自動試験では環境変数で接続先を指定できます。

```bash
MIRAKURUN_SERVER=http://192.168.3.3:40772 \
MIRAKURUN_SERVICE_ID=3203246080 \
MIRAKURUN_AUTOPLAY=1 \
QT_QPA_PLATFORM=xcb ./build/mirakurun-viewer
```

## 長時間試験

`scripts/monitor-memory.sh <PID> [interval]`でRSS、PSS、FD数、スレッド数をCSVへ記録できます。

NHK大津を同じ条件で比較するには、`scripts/benchmark-player.sh mpv`または
`scripts/benchmark-player.sh gstreamer`を実行します。既定の測定時間は10分で、
結果は開始日時ごとの`benchmark/`ディレクトリに保存されます。
ウォームアップ後のメモリーが時間比例で増えないことを判定してください。
