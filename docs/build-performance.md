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
型検査だけなら`bash scripts/build/with-build-lock.sh cargo check --manifest-path rust/Cargo.toml --locked`
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
Cargo診断を直接使う場合も`bash scripts/build/with-build-lock.sh cargo ...`を使ってください。

Qt/C++生成では、Qt連携のRustファイルとヘッダーを個別に監視します。
`rust/src`全体の監視を外すことで、Qt非依存のRustファイルを編集しても
C++やQMLのコンパイル結果を再利用できます。Qt連携・ヘッダー・QMLの変更時は再生成します。

`dev`は差分コンパイルを有効にし、最適化を省いて編集後のコンパイル時間を抑えます。
デバッグ情報はファイル名と行番号を残します。変数を調べる場合は
`CARGO_PROFILE_DEV_DEBUG=2`を指定します。
`release`の最適化・デバッグ情報の設定は維持しています。
独立crateのReleaseテストも、共通Cargo設定でアプリと同じ`debug=1`に揃えています。
以前は独立crateが既定の`debug=0`を使うため、同じ依存関係にも別のコンパイル結果が必要でした。
設定の意味は[Cargoのプロファイル](https://doc.rust-lang.org/cargo/reference/profiles.html)を参照してください。
再生速度やフレーム時間の評価には`release`を使います。

## 並列数とCIキャッシュ

ローカルのビルド並列数はCargoのCPU検出に任せます。
メモリー使用量を抑える場合は`CARGO_BUILD_JOBS=2`などで指定してください。
nextestの並列数は引き続き2です。実時間を使う再生試験は単独実行し、
ビルドの並列化によって試験どうしが干渉する範囲は広げません。
CIのCargo・AppImage・deb・Flatpakも、2並列固定から実行ホストの`nproc`へ変更しています。
この設定はコンパイルの並列数だけに適用します。

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
`CCACHE_DISABLE=1 bash scripts/build/with-build-lock.sh cargo build --manifest-path rust/Cargo.toml --locked`
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

CIではUbuntu 24.04と26.04を分け、Cargo生成物とccacheも別々にキャッシュします。
Cargoのキーは実際のDockerイメージIDと設定・依存情報で決め、コミットだけが変わった場合は
同じキャッシュを再利用します。ccacheは500 MBを上限としてコミットごとに更新します。
保存は`main`上で行い、PR・タグ・作業ブランチでは共有キャッシュを復元します。
実行ファイルとincrementalデータは保存対象から外します。
復元後はリポジトリー内のcrateとローカルパッチだけをCargoでcleanし、外部依存とccacheを残します。
ソースの時刻が生成物より古くても、以前の実装を誤って再利用しないためです。
キャッシュの構成と配布手順は[CIとリリース](ci-release.md)を参照してください。
復元キーの扱いは[actions/cacheの公式資料](https://github.com/actions/cache/blob/main/caching-strategies.md)に従います。

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
CARGO_TARGET_DIR=build/cargo bash scripts/build/with-build-lock.sh \
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

## CIでの確認（2026-09-27）

Ubuntu 24.04の共通テストランナーが記録した工程の合計は、
[最初の実行（`9d68bda`）](https://github.com/ouvill/NagameTV/actions/runs/36273744055)が
1,157.93秒、[追加変更後（`5182558`）](https://github.com/ouvill/NagameTV/actions/runs/36276851941)が
765.12秒でした。両方ともCPU試験はすべて成功しています。
Docker環境の準備、キャッシュ転送、実EPGStationの結合テスト、配布物生成はこの合計に含みません。

最初の実行は2並列でコンパイル結果のキャッシュがなく、追加変更後は4並列で
外部依存・ccacheを復元し、QML生成を並列化しています。
各1回の比較で、個々の変更の効果を分離した数値ではありません。
実行ログと工程別の集計は`build/throughput-study/latest-ci-tests/`に保存しています。

## CI履歴の追加調査（2026-10-05）

2026-09-28〜10-01（UTC）の成功した12実行を対象に、GitHub Actionsのステップ時間と
共通テストランナーのログを調べました。変更内容やキャッシュ状態が異なるため、
この範囲の最短・最長を改善前後の比較には使いません。

| 工程 | 最短 | 最長 |
| --- | ---: | ---: |
| Ubuntu 24.04のDocker環境準備 | 66秒 | 1,012秒 |
| CPUテスト工程全体 | 309秒 | 1,162秒 |
| CPUテスト工程内のコンパイル合計 | 208秒 | 1,000秒 |
| AppImage生成・検証 | 150秒 | 310秒 |
| 実EPGStationとの結合試験 | 153秒 | 195秒 |
| GitHub Releaseの作成・更新ジョブ（実行された11件） | 20秒 | 50秒 |

[10月1日のmain実行](https://github.com/ouvill/NagameTV/actions/runs/36929815266)では
CPUテスト工程内のコンパイルが約1,000秒、Rust・Qtのテスト実行とCLI検査が約57秒でした。
Docker環境を作り直した[v0.2.1の実行](https://github.com/ouvill/NagameTV/actions/runs/36914844274)では、
nextestのソースからの導入だけで約181秒かかっています。
この12実行ではUbuntu 24.04のテスト・AppImage・debジョブが最後まで残り、
全体の待ち時間を決めていました。Releaseの作成・更新よりも、その前のビルドと検証が支配的です。

[別のmain実行](https://github.com/ouvill/NagameTV/actions/runs/36909139143)では、
debの導入試験が1,277秒かかりました。その後追加されたaptキャッシュも、
10月1日の直近のmain実行ではroot所有の`lock`と`partial`を読めず保存に失敗していました。
保存対象を`archives/*.deb`へ絞り、取得済みパッケージを次回へ残せるように修正しています。

Cargoキャッシュにも、globで選んだディレクトリーをtarが再帰的に保存すると
除外指定が効かない問題がありました。実行ファイルとincrementalの除外は
GNU tarの`TAR_OPTIONS`へ移しています。復元後のローカルcrateのcleanもdev・releaseの
両方を明示し、変更されたソースの時刻が古くてもReleaseの実装を再コンパイルさせます。

この調査に基づき、nextestの導入を検証済みの公式バイナリーへ変更し、
アプリと独立crateのRelease設定を統一しました。Ubuntu 24.04の一時コンテナーで、
ダウンロード・SHA-256検証・展開・バージョン確認は4.81秒で完了しました。
これはWorkshopでの1回の値で、GitHubの181秒との条件を揃えた比較ではありません。
Release設定の統一は、同じ外部依存を持つ2つの独立crateを実際にビルドする回帰試験で確認します。
コンパイル結果の数と更新時刻から、2つ目のcrateでその依存が再コンパイルされないことを検査します。

### 同じアプリのソースでの比較

キャッシュ設計変更後の[`83f1067`](https://github.com/ouvill/NagameTV/actions/runs/37226792693)と、
nextest導入・Release設定を変更した[`2059a5e`](https://github.com/ouvill/NagameTV/actions/runs/37227572385)を、
それぞれ手動実行しました。アプリのソースは同じで、Cargo・ccacheは両方とも未ヒットです。
各1回、別のGitHubホストランナーでの測定です。

| 工程 | 変更前 | 変更後 |
| --- | ---: | ---: |
| Ubuntu 24.04のDocker環境準備 | 427秒 | 204秒 |
| Docker環境準備内のnextest導入レイヤー | 194.2秒 | 0.5秒 |
| CPUテスト工程全体 | 1,008秒 | 1,235秒 |
| CPUテスト工程内のコンパイル合計 | 870.26秒 | 1,061.85秒 |
| `viewer-comments`のテスト準備でコンパイルしたcrate数 | 179 | 84 |

CPU試験は両方とも51工程すべて成功しました。環境準備は223秒短縮し、
Release設定の統一で重複コンパイルも減りましたが、CI全体の短縮はこの比較では確認できていません。
変更後はアプリ本体のコンパイルも515.37秒から647.97秒へ伸びており、
設定変更の効果と実行環境のばらつきは切り分けていません。
mainに共有キャッシュが作成された後の所要時間は別途測定が必要です。

この比較実行には、その後追加したapt・Cargoアーカイブの修正とReleaseのclean修正は含みません。
これらは実際の`@actions/cache`で作ったアーカイブと、dev・release両方での回帰試験で検証しています。
両方の実行でFlatpak、Ubuntu 26.04のdeb、実EPGStationとの結合試験も成功しましたが、
AppImage生成後のUbuntu 24.04向けdeb変換はQt Positioningの不足により失敗しました。
配布SDKへ`qtpositioning`、OS側へ`libxkbfile1`を追加し、アプリのコンパイル前に
QtWebEngineの共有ライブラリー不足を検出するようにしています。
QML経由で必要になるQtWebEngineの補助プロセス・リソースも明示的に同梱し、必須ファイルを検査します。

修正後のDockerイメージをWorkshopで構築し、比較用CIで作った実行ファイルと
インストール資源を使い、依存ライブラリーを新しいSDKから収集し直しました。
AppImageの348 ELFすべてがglibc 2.39の上限検査に通り、deb変換、クリーンなUbuntu 24.04での
導入・全ELFのリンク・整合性・削除の検査も成功しました。最終変更でのtooling試験89件、
actionlint、変更したシェルのShellCheckも成功しています。
これは配布工程を対象にした再検証で、最終変更を含むGitHub Actions全体の再実行はしていません。

調査ログと工程別データはGit対象外の`build/ci-cache-study/`に保存しています。
