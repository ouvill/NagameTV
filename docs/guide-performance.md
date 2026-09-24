# 番組表の表示遅延

2026-09-25に、番組表を開く・日付を変える・放送波を変える際の表示処理を改善した。
取得済みEPGを使う条件でも、表示範囲外の番組の文字組みや、放送波変更中の列の再生成に
時間がかかっていた。

[GuideTimeline.qml](../rust/qml/GuideTimeline.qml)は横方向のListViewで必要な局の列を生成し、
左右1列を先読みする。各列では、画面内と上下半画面分（最低60分）に重なる番組だけ
Loaderを有効にする。画面外のセルは複数フレームに分けて生成する。
セルでは描画に必要なモデルの項目を直接参照し、番組オブジェクトを取り出すlookupは
クリック時に行う。
日付変更時のHTTP要求やEPGの更新間隔は変更していない。

縦横の操作は外側のFlickableに集約する。ListViewの原点は行の追加・削除で動くため、
スクロール位置にはoriginXを加える。この適用には遅延Bindingを使い、列の配置中に
contentXを再帰的に更新しない。
[QtのFlickable仕様](https://doc.qt.io/qt-6/qml-qtquick-flickable.html#contentX-prop)と
[Binding.delayed](https://doc.qt.io/qt-6/qml-qtqml-binding.html#delayed-prop)を参照。

表示部品の解放後も、Rustのモデルには当日の全番組が残る。詳細カードとキーボード操作は
番組の識別子から参照するため、画面外へ移動しても選択を失わない。
部品テストでは、生成数の制限、縦横スクロール後の詳細保持、画面外番組へのキー操作、
放送波・表示局フィルター変更後の配置とクリック先を確認する。

## 計測方法

[計測用QML](../rust/qml/benchmarks/tst_GuidePerformance.qml)は製品のモデルと番組表を使う。
1280×720、地上波27局・BS27局、1局1日48番組、長い説明文を持つ合成データで、
各操作から最初の描画までを7回測る。全表示列に番組セルがあり、位置と局が正しいことも確認する。
日付変更の値にはfixtureのJSON解析とモデル構築が含まれ、実サーバーの通信時間と
アプリ全体の開閉アニメーションは含まれない。

```sh
CARGO_TARGET_DIR=build/cargo SQLX_OFFLINE=true cargo build --manifest-path rust/Cargo.toml --release --locked --features native_tests
python3 scripts/run-gui-tests.py -- env QT_QPA_PLATFORM=xcb QSG_RHI_BACKEND=opengl \
  build/cargo/release/nagametv --native-tests channel-views \
  -input rust/qml/benchmarks/tst_GuidePerformance.qml
```

公開ランチャーで専用画面・実GPU・仮想音声出力を検証し、他のビルドやGUIテストを
並行実行せずに測る。実行時間に固定の合否閾値は設けず、同じ環境とfixtureで比較する。

## 計測結果

初回の表示改善時点（以下のスクロール調整前）の計測。
Qt 6.10.2、NVIDIA GeForce RTX 4070 Ti、専用Xwaylandでの中央値。
変更前はコミット`80705e5`の実装を使った。

| 操作 | 変更前 | 変更後 |
| --- | ---: | ---: |
| 番組表の生成から初回描画まで | 930 ms | 154 ms |
| 日付変更から描画まで（fixture構築を含む） | 838 ms | 231 ms |
| 地上波からBSへの変更から描画まで | 3,291 ms | 261 ms |
| 初回描画時に生成された番組セル数 | 432 | 64 |

fixtureの解析・モデル構築だけの中央値は、変更前53 ms、変更後49 msだった。
同じ入力で画面部品の生成を減らした結果であり、実サーバーや利用者の環境での待ち時間を
保証する数値ではない。計測ログはGit対象外の`build/guide-perf-before.log`と
`build/guide-perf-after.log`に保存した。

検証ではQML部品330件が成功し、評価用機能の18件は対象外だった。製品Main.qmlの
起動・番組表の開閉・終了、全92ファイルのqmllint、UIスタイル検査、通常版のビルドも成功した。
番組表の画像は`build/navigation-review/guide-1440.png`と`guide-640.png`で確認した。
検証ログは`build/guide-perf-qml-suite.log`、`guide-perf-startup.log`、
`guide-perf-qmllint.log`、`guide-perf-release-build.log`に保存した。

## 縦スクロール時の先読み

表示範囲を絞るだけでは、スクロールで新しい時間帯に入るたびに複数局のセルをまとめて
生成する処理が残る。画面外のLoaderは`asynchronous`を有効にし、生成を複数フレームに
分散する。先読み範囲も上記の半画面分まで広げ、表示前に用意する時間を確保した。
スクロールバーなどで遠くへ飛んだ場合は、表示範囲のセルを同期的に完成させて空白を防ぐ。
これはQtの[Loader.asynchronous](https://doc.qt.io/qt-6/qml-qtquick-loader.html#asynchronous-prop)を使う。

セル内の文字はLabelからTextへ変更し、ApplicationWindowのフォントファミリーを明示的に渡す。
番組表を単体で使う場合はApplicationのフォントファミリーを使う。色・文字サイズ・折り返し・
セル内のクリッピングは維持する。

部品テストでは先読み済みセルの保持、ホイール相当の位置変更と大きな位置ジャンプでの
表示準備、先読み中のEPG更新・放送波変更後の選択先を確認する。

[スクロール計測用QML](../rust/qml/benchmarks/tst_GuideScrolling.qml)では、1920×1080、
54局・1局48番組の合成データを使い、縦・横・斜めに各5秒で往復する。
計測中にQtTestの待機関数を呼ばず、FrameAnimationのフレーム間隔を記録する。
往復の端では、各表示列の番組が生成済みであることも確認する。

```sh
python3 scripts/run-gui-tests.py -- env QT_QPA_PLATFORM=xcb QSG_RHI_BACKEND=opengl \
  build/cargo/release/nagametv --native-tests channel-views \
  -input rust/qml/benchmarks/tst_GuideScrolling.qml
```

前述のテスト用ビルドを使い、他のビルド・GUI試験が終わってから測る。
この計測は位置をプログラムから連続的に動かすもので、物理ホイールやタッチパッドの
イベント到着から画面表示までの遅延を測るものではない。描画間隔には専用画面環境の
同期も含まれるため、実ディスプレイのフレームレートとしては扱わない。

同じ専用画面環境で、初回の表示改善後を基準に1回、スクロール調整後を3回計測した。
各回・各方向の計測時間は往復合わせて10秒。変更後の欄は3回の最小値から最大値を示す。

| 方向・指標 | 調整前（1回） | 調整後（3回） |
| --- | ---: | ---: |
| 縦・描画間隔の中央値 | 25.1 ms | 25.2 ms |
| 縦・99パーセンタイル | 27.4 ms | 27.3–27.9 ms |
| 縦・最大描画間隔 | 61.2 ms | 28.8–29.1 ms |
| 横・最大描画間隔 | 30.9 ms | 30.6–49.5 ms |
| 斜め・99パーセンタイル | 52.1 ms | 36.2–39.8 ms |
| 斜め・最大描画間隔 | 53.4 ms | 41.8–55.8 ms |

縦方向で50 msを超えた間隔は調整前の10秒で1回、調整後の合計30秒では0回だった。
中央値と99パーセンタイルはほぼ同じで、今回の観測だけで通常時のフレームレート向上や
利用者の環境での改善幅は判断できない。横方向の改善も確認できていない。
ログは`build/guide-scroll-before.log`と`build/guide-scroll-final-1.log`から
`guide-scroll-final-3.log`に保存した。

調整後に初回表示の計測も再実行し、中央値は生成151 ms、日付変更202 ms、
放送波変更265 msだった。ログは`build/guide-scroll-opening.log`に保存した。
QML部品331件が成功し、評価用機能の18件は対象外だった。製品画面の起動・番組表の
開閉・終了、全92ファイルのqmllint、UIスタイル検査とその7件のテスト、通常版ビルドも
成功した。検証ログは`build/guide-scroll-qml-suite.log`、`guide-scroll-startup.log`、
`guide-scroll-qmllint.log`、`guide-scroll-release-build.log`に保存した。
