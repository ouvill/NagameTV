# ビルド・テストの待ち時間を減らす

通常開発は`dev`、配布とCIは`release`を使います。既存のWorkshopにも次の設定を適用できます。
コンテナーの再作成や追加パッケージの導入は不要です。

```sh
cmake -S . -B build -DCMAKE_BUILD_TYPE=Debug -DNAGAMETV_DISTRIBUTION=OFF
cmake --build build
python3 scripts/test.py app
python3 scripts/test.py viewer-comments --filter 'test(cache::)'
```

変更範囲に応じて[開発手順](development.md#テストと診断)の試験を選びます。
全CPU試験は`python3 scripts/test.py cpu`、最適化した構成での検証は
`python3 scripts/test.py cpu --profile release`です。
初めて`dev`へ切り替えるときは依存物をコンパイルします。
型検査だけなら`bash scripts/with-build-lock.sh cargo check --manifest-path rust/Cargo.toml --locked`
を使えます。CMake・共通ランナー・直接のCargo診断は`build/cargo`を共用し、
同じ依存物を別の出力先で作り直すことを避けます。`CARGO_TARGET_DIR`による変更は可能です。

## 再コンパイルを減らす

以前はビルド情報の時刻を毎回更新していたため、ソースが同じでもアプリを再コンパイルしていました。
現在はビルドロックを取った後にGitの状態とソース内容を取得し、内容のハッシュをCargoへ渡します。
同じ入力の再実行では元の実行ファイルとそのビルド日時を再利用します。
未追跡ファイルの追加・削除、サブモジュール、linked worktreeも対象です。
Git情報のないアーカイブとラッパーを通さないCargo実行では、毎回更新する動作を維持します。

この判定は[Cargoのビルドスクリプトの変更検知](https://doc.rust-lang.org/cargo/reference/build-scripts.html#change-detection)を使います。
`build/`などの無視対象は入力に含めず、ビルド出力の更新で次のビルドが始まる循環を避けます。
Cargo診断を直接使う場合も`bash scripts/with-build-lock.sh cargo ...`を使ってください。

Qt/C++生成では、Qt連携のRustファイルとヘッダーを個別に監視します。
`rust/src`全体の監視を外すことで、Qt非依存のRustファイルを編集しても
C++やQMLのコンパイル結果を再利用できます。Qt連携・ヘッダー・QMLの変更時は再生成します。

`dev`は差分コンパイルを有効にし、最適化を省いて編集後のコンパイル時間を抑えます。
デバッグ情報はファイル名と行番号を残します。変数を調べる場合は
`CARGO_PROFILE_DEV_DEBUG=2`を指定します。
`release`の最適化・デバッグ情報の設定は維持しています。
設定の意味は[Cargoのプロファイル](https://doc.rust-lang.org/cargo/reference/profiles.html)を参照してください。
再生速度やフレーム時間の評価には`release`を使います。

## 並列数とCIキャッシュ

ローカルのビルド並列数はCargoのCPU検出に任せます。
メモリー使用量を抑える場合は`CARGO_BUILD_JOBS=2`などで指定してください。
nextestの並列数は引き続き2です。実時間を使う再生試験は単独実行し、
ビルドの並列化によって試験どうしが干渉する範囲は広げません。

CIではUbuntu 24.04と26.04のコンパイル結果を別々にキャッシュします。
実際のDockerイメージIDとCargoの設定・依存情報をキーに含め、同じ環境の過去の結果を再利用します。
実行ファイルとincrementalデータは保存対象から外します。
キャッシュの構成と配布手順は[CIとリリース](ci-release.md)を参照してください。
復元キーの扱いは[actions/cacheの公式資料](https://github.com/actions/cache/blob/main/caching-strategies.md)に従います。
リモートCIでの短縮量は、変更反映後の実行で確認する必要があります。

## moldとその他の候補

Rust 1.90以降の`x86_64-unknown-linux-gnu`は標準でLLDを使います。
このプロジェクトのCXX-Qtも利用可能なLLDを選びます。
[Rust公式発表](https://blog.rust-lang.org/2025/09/18/Rust-1.90.0/)を参照してください。
moldの採用にはCargoビルド全体での評価と動作確認が必要です。
今回、既定リンカーは変更していません。

moldを別途評価するときは、[公式のRust向け設定](https://github.com/rui314/mold#how-to-use)に従い、
同じツールチェーン・入力・並列数で、Cargoビルド全体と実行時の動作を比較します。
`RUSTFLAGS`の変更は依存物のキャッシュも無効にするため、最初のビルドと反復時を分けて記録します。
比較用の`CARGO_TARGET_DIR`を用意し、通常開発のキャッシュを切り替え続けないようにします。

sccacheはRustのバイナリーリンクとincremental有効時のコンパイルをキャッシュできません。
今回はローカルで差分コンパイル、CIでCargoの生成物の保存を使います。
[sccacheのRust対応範囲](https://github.com/mozilla/sccache/blob/main/docs/Rust.md)を参照してください。
workspaceへの統合やクレート分割は依存機能・lockfile・配布構成に影響するため、
今回の変更には含めていません。

## ビルド全体を測る

変更なしの再実行、Rust変更後、QML変更後、空の出力先からのビルドを分けます。
`cargo clean`で普段のキャッシュを消さず、初回計測には別の出力先を使ってください。
計測中にソースを編集したり、他のコンパイルを動かしたりしないようにします。

```sh
/usr/bin/time -f 'elapsed=%e maxrss_kb=%M' cmake --build build
CARGO_TARGET_DIR=build/cargo bash scripts/with-build-lock.sh \
  cargo build --manifest-path rust/Cargo.toml --locked --timings
```

Cargoのタイミングレポートは`build/cargo/cargo-timings/`、共通テストランナーの
各工程の所要時間は`build/test-runs/run-*/summary.json`に保存されます。
環境、プロファイル、初回か反復か、変更した入力、試行回数を併記してください。
[Cargo公式のビルド性能ガイド](https://doc.rust-lang.org/cargo/guide/build-performance.html)も参照できます。

## Workshopでの確認結果（2026-09-27）

Ubuntu 26.04、Rust 1.98.1、利用可能CPU数16の既存キャッシュで、
`cmake --build build`の開始から終了までを測りました。変更前の並列数は2、変更後はCargoの自動検出です。
Releaseは配布用featureあり、devは通常の開発構成です。

| 操作 | 変更前 | 変更後 |
| --- | ---: | ---: |
| Release、ソース変更なし | 33.29秒（1回） | 0.67秒（3回の中央値） |
| dev、ソース変更なし | 未計測 | 0.62秒（3回の中央値） |
| dev、Qt非依存のRustファイルへコメント1行を追加 | 未計測 | 5.32秒（1回） |

変更なしの試行はすべて実行ファイルのSHA-256が一致しました。
Rustの小変更ではQt/C++の生成物の更新時刻とサイズが変わらないことを確認し、計測後にソースを復元しました。
初回ビルド、実装の大きな変更、QML編集、リモートCIの短縮量を示す数値ではありません。
ログと計測条件はGit対象外の`build/build-performance-20260927/`に保存しています。
