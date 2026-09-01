# Mirakurun Viewer

Qt 6/QML のUIとRustの再生コアを組み合わせたMirakurunライブ視聴クライアントです。
Mirakurunの `/api/services/{id}/stream` をlibmpvで直接再生します。

## 設計上のメモリー境界

- libmpvの生ポインタと`unsafe`は `rust/src/lib.rs` 内だけに限定
- Rustの`Drop`で描画コンテキストを先に、mpvハンドルを後に破棄
- libmpvイベントは50msごとに空になるまで処理し、アプリ側に蓄積しない
- demuxerの前方キャッシュを64 MiB、後方キャッシュを0に制限
- QML側に映像フレームやストリームバイト列をコピーしない
- チャンネル再読み込みは同じPlayerへ`loadfile replace`を送り、Playerを増殖させない

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

`project-mirakurun-viewer` SDKのhealth checkは、Rust、CMake、Qt 6、libmpvを
refreshのたびに検証します。GUI、GPU、PulseAudioは既存の`project-gui` SDKと
`.workshop/dev.yaml`の接続定義から提供されます。

### Ubuntuへ直接導入する場合

Ubuntu 24.04:

```bash
sudo apt install build-essential cmake ninja-build rustc cargo \
  qt6-base-dev qt6-declarative-dev qml6-module-qtquick \
  qml6-module-qtquick-controls qml6-module-qtquick-layouts \
  qml6-module-qtquick-templates \
  libmpv-dev qt6-wayland
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build build
```

実行:

```bash
./build/mirakurun-viewer
```

## 長時間試験

`scripts/monitor-memory.sh <PID> [interval]`でRSS、PSS、FD数、スレッド数をCSVへ記録できます。
ウォームアップ後のメモリーが時間比例で増えないことを判定してください。
