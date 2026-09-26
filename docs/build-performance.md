# ビルド・テストの待ち時間を減らす

通常開発は`dev`、配布とCIは`release`を使います。既存のWorkshopにも次の設定を適用できます。
基本設定の切り替えにコンテナーの再作成は不要です。

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

### C++のコンパイル結果を再利用する

`ccache`があれば、共通ラッパーがネイティブ用の`HOST_CC`・`HOST_CXX`へ設定します。
QMLやQt連携のRustファイルを変更して生成処理が再実行されても、入力が同じC++の
コンパイル結果を再利用します。初回はキャッシュを作るため、短縮できるとは限りません。
Workshop・FedoraのセットアップとCIイメージには導入手順を含めています。
既存のUbuntu環境へ追加する場合は次を実行します。

```sh
sudo apt-get install --yes --no-install-recommends ccache
cmake --build build
CCACHE_DIR="$PWD/build/ccache" ccache --show-stats
```

保存先は`build/ccache`で、`CCACHE_DIR`で変更できます。
Cargoの出力先を作り直してもキャッシュを残せます。
ccacheがなければ通常のコンパイルを使い、無効化して比較する場合は
`CCACHE_DISABLE=1 bash scripts/with-build-lock.sh cargo build --manifest-path rust/Cargo.toml --locked`
とします。

明示した`CC`・`CXX`・`HOST_CC`・`HOST_CXX`とターゲット別の指定を優先し、
クロスコンパイラーの選択はcc-rsに任せます。これらの環境変数はラッパーの起動前に設定してください。
独自コンパイラーにもキャッシュを使う場合は、たとえば`CXX="ccache clang++"`を指定します。
Rustの差分コンパイルと、QMLの事前コンパイルは維持します。
ヘッダーの時刻検査を省く`sloppiness`などは設定しません。
動作条件は[ccacheの説明](https://ccache.dev/manual/4.12.3.html)と
[cc-rsの環境変数](https://docs.rs/cc/latest/cc/#external-configuration-via-environment-variables)を参照してください。

QMLの事前コンパイルは、ファイルごとにCargoのjobserverから追加の実行枠を借りて並列化します。
追加の枠がなければ呼び出し元で処理するため、`CARGO_BUILD_JOBS=1`でも停止しません。
型情報の生成を先に完了し、各QMLの処理後に入力順を維持してローダーを生成します。
コンパイルオプションやQMLの型検査は維持します。
この並列数の調整は[Cargoのビルドスクリプトの規約](https://doc.rust-lang.org/cargo/reference/build-scripts.html#jobserver)
に従います。

CIではUbuntu 24.04と26.04のCargo生成物とccacheを別々にキャッシュします。
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

### フルビルドとコード変更後の追加比較

同じWorkshopで、Rust 1.98.1、GCC 15.2.0、Qt 6.10.2、ccache 4.12.3、
`dev`、`CARGO_BUILD_JOBS=16`に固定し、共通ラッパーを含む
`cargo build --manifest-path rust/Cargo.toml --locked --timings`全体を測りました。
依存ソースは取得済みです。比較開始点の`2bf718f`には、上記のプロファイル変更、
変更検知の修正、並列数の制限解除がすでに含まれています。

| 条件 | 追加変更前 | ccacheのみ | ccacheとQML並列化 |
| --- | ---: | ---: | ---: |
| 空のCargo出力先、ccache無効 | 154.72秒 | — | 141.28秒 |
| 空のCargo出力先、ccacheも空 | — | 160.31秒 | 151.40秒 |
| 空のCargo出力先、ccacheを再利用 | — | 82.36秒 | 71.21秒 |
| QMLの色1項目を変更（3回の中央値） | 62.57秒 | 40.50秒 | 30.49秒 |
| Qt非依存のRust定数を変更（3回の中央値） | 未計測 | 未計測 | 4.93秒 |

フルビルドは各1回の参考値です。初回はccacheへの保存にも時間がかかります。
再利用の試行ではCargo出力を別名へ退避し、同じパスを空にして作り直しました。
Rustのコンパイル結果は再利用していません。71.21秒はC++キャッシュがある場合の結果で、
キャッシュがすべて空の初回ビルドの値ではありません。

QMLでは`rust/qml/Theme.qml`の`canvas`を異なる3色へ変更しました。
各試行は追加変更前が61.04／62.57／88.55秒、ccacheのみが40.55／40.50／39.76秒、
QML並列化後が52.67／30.34／30.49秒です。試行間にばらつきがあります。
変更ごとに対応するC++オブジェクトのSHA-256が変わることを確認しました。
元のQMLから生成した92ファイルは、並列化前後でバイト単位で一致しています。

Rustでは`rust/src/settings/mod.rs`の設定ファイルサイズ上限を64から65、66、67 KiBへ変更し、
4.97／4.93／4.87秒でした。アプリのネイティブ生成物7,577ファイルの更新時刻とサイズは
変わりませんでした。計測に使ったRust・QMLの変更は復元しています。

Clang 21.1.8をC/C++の両方へ指定したキャッシュなしのフルビルドも試しましたが、
181.94秒（1回）で、この試行では短縮を確認できなかったため既定のコンパイラーは変更していません。
環境情報、各ビルドのログ、計測用スクリプト、Cargoのタイミングレポートは
Git対象外の`build/throughput-study/`に保存しています。
