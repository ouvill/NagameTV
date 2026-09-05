# libaribcaption Rust crates

- `libaribcaption-sys`: 同じソースからC++ライブラリをビルドし、インストール済みCヘッダーからbindgenでFFIを生成する。生成コードはOUT_DIRのみ。
- `libaribcaption`: デコーダーとコンテキストを所有し、Dropで解放する安全なRust API。結果はRust所有の文字列・領域・文字・色・時刻情報。Qt/GStreamer/Mirakurunには依存しない。
- アプリの`subtitles/decoder.rs`: Rustの字幕データからQML用モデルへの変換のみ。PTSと表示時間を保持し、色をQML用文字列に変換する。表示・消去の判定はアプリの`subtitles/timing.rs`で映像の再生時刻に合わせて行う。

現時点では日本語JIS・Aプロファイル・第1言語のデコード部分を公開する。レンダラー、DRCSビットマップ取得など、上流API全体の安全なラッパーではない。

## ビルドとテスト

CMake、C/C++コンパイラー、libclangが必要。Ubuntuでは`libclang-dev`を導入する。検出できない場合は`LIBCLANG_PATH`で指定する。

```sh
cargo test --manifest-path rust/Cargo.toml -p libaribcaption-sys -p libaribcaption --release
```

Qtや表示デバイスなしで実行できる。上流の字幕サンプルを用いたデコード・所有権の回帰テストを含む。

ネイティブソースは既存の`third_party/libaribcaption`サブモジュールを利用する。`ARIBCAPTION_SOURCE_DIR`で別のソースディレクトリを指定可能。
独立したリポジトリに移す際はこのソースの配置/同梱方法も設定すること。現時点ではworkspace内のpath依存であり、外部公開はしていない (`publish = false`)。

C++ランタイムはccクレートにターゲットに応じた選択を任せる。Windows/macOSを考慮したビルド構成だが、実行検証済みなのはLinuxのみ。クロスコンパイルでは対象のCMakeツールチェーンとClangのsysroot等の設定が別途必要。
