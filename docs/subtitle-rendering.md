# 字幕の書体・縁取りの移植

mainの `qml/Main.qml` と `rust/src/subtitle_outline.h` を参照して、
標準Text.Outlineから同梱ARIBフォントと文字輪郭の描画へ移行した。

## 責務と描画

- `features/subtitles` はARIBデコード・PTS同期・表示期限・上限付き待機列を担当する。
- `player/subtitle_rendering.rs` はQtのQFontとQStringを受ける境界。字幕の解析状態を参照しない。
- `subtitle_outline.h` はQPainterPath.addTextからSVGパスを作る。呼出しごとに値を返し、
  パス・文字・フォントの履歴やキャッシュを所有しない。
- `SubtitleOverlay.qml` は字幕面の座標・背景・文字だけの字幕表示とフォント読込を担当する。
- `SubtitleGlyph.qml` はmainと同じLabelのレイアウト、基準線、文字サイズの6%を半径とする
  輪郭、横方向の圧縮を担当する。輪郭と文字は同じScaleを通す。

mainと同じRounded M+ 1m for ARIBをQtリソースへ同梱し、ライセンスも同梱する。
通常はこのフォントを使い、読込失敗時はログに記録してmain同様Noto Sans CJK JPを指定する。
これはフォントの表示方針であり、GPUやディスプレイが使えない場合の代替実装ではない。

[QPainterPath.addText](https://doc.qt.io/qt-6/qpainterpath.html#addText)は文字の基準線を原点として
輪郭を生成する。小さい仮名や長音記号のインク上端へ正規化せず、負のy座標も保持する。
QPainterPathのcubicはCurveToと二つのCurveToDataで構成されるため、三要素をまとめて変換する。
[PathSvg](https://doc.qt.io/qt-6/qml-qtquick-pathsvg.html)でShapeに渡し、
[Shape.CurveRenderer](https://doc.qt.io/qt-6/qml-qtquick-shapes-shape.html#preferredRendererType-prop)を指定する。
Shape.CurveRendererにはQt 6.6以降が必要。

## メモリーと更新

輪郭用のLoaderはstrokedの文字にだけ有効。文字・書式・サイズが変わった時にパスを作り、
再生の毎フレームや字幕pollごとには作らない。QMLのBindingで更新し、追加タイマーは持たない。
字幕の置換・消去時に以前のdelegateを破棄する。字幕無効時と表示OFF時にはmain側のLoaderが
SubtitleOverlayごと破棄し、字幕の描画に伴う呼出しを止める。

同梱フォントの[FontLoader](https://doc.qt.io/qt-6/qml-qtquick-fontloader.html)もOverlayに所属するが、
Qtのフォント・グリフキャッシュやアロケーターの保持領域がその時点で全てOSへ返るとは限らない。
この変更だけでRSSの減少や長時間のリーク不存在を保証しない。
字幕待機列の既存上限（128画面、1画面2048セル・文字列上限）は維持する。

## 検証方法

`scripts/test-subtitle-outline.sh` はCPU上のQImageとQCoreApplicationだけで実行する。
矩形、小さい輪郭、長音相当の棒、曲線、負のbearingとdescender、複数contour、空のパスを
直接描いた結果とSVGに変換して描いた結果を比較し、座標変換によるずれを検出する。
ディスプレイ・GPU・音声は使用しない。

`scripts/test-subtitle-rendering.sh` はDISPLAYとGPUデバイスを検出し、xdpyinfoとglxinfoで
実際に検証してからX11/OpenGLのQt Quick Testを起動する。欠落・検証失敗時は終了し、
headlessやsoftware rendererに自動変更しない。テスト用の一時ビルドは終了時に削除する。
Qtのテストproviderも本番のsubtitle_outline.hを呼び、描画結果を模擬しない。

Qtの実描画で、小さい仮名・長音・descender・漢字に文字色と縁の色のピクセルが存在することを
確認する。基準線、同梱フォント、半分への拡縮、縁取りなしでパスを生成しないこと、
字幕が変わらない間の再生成がないこと、字幕消去・Loader無効化を確認する。
Qt内部のキャッシュ量や全メモリーの解放を測るテストではない。

2026-09-07: Rust 40件成功（外部TS依存1件は未実行）、輪郭座標のC++試験成功、
Qt描画7ケース成功（初期化と終了を含む9件）、qmllint警告なし、Clippy警告なし、ビルド成功。
最初の描画試験はTestCaseを非表示のままにしたため失敗し、テスト画面をvisibleにして再検証した。

実アプリは表示・NVIDIA OpenGL・PulseAudioの動作確認後、`--features=subtitles`で起動し、
実サーバーの一覧取得、自動再生、正常終了を確認した。フォント読込やQMLのエラーは出ていない。
再生後約35秒待機したが字幕受信数は0だったため、この試験を実放送字幕の描画成功とは数えない。
証跡はGit対象外の `benchmark/subtitle-rendering/smoke.py`、`smoke.log`、`playing-*.png`。
受信する放送での確認、全機能併用・長時間メモリー測定は未完了。


## 放送サービスの選択

字幕Sessionにも音声・EPGと同じBroadcastServiceを渡す。HTTP配信用idからの
剰余計算を廃止し、PATのprogram_numberにはサービスAPIの明示的なserviceIdを使う。
[MirakurunのService定義](https://github.com/Chinachu/Mirakurun/blob/master/api.d.ts)でも
idとserviceIdは別のフィールドである。networkId/serviceIdが取得できない場合は
MissingServiceエラーを字幕状態に表示し、callback・probeを登録する前に終了する。
映像再生は既存の字幕失敗経路に従って続けられる。停止成功後にSessionを破棄する寿命は保つ。

CPU回帰試験はid=777、networkId=4、serviceId=42のサービスJSONから実際のSession用
パーサーを構成する。サービス7と42を持つ生成PATをTSパケットとして渡し、
42に対応するPMT PIDだけが登録されることと、情報欠落が型付きエラーになることを確認する。
受信機器や映像・音声出力は使用しない。この試験は実放送の字幕描画や選局の長時間試験を
代替するものではない。以降の節に字幕parserと時計のMutex異常処理を記録する。

## 購読管理の終了状態とMutex

Subscriptionsの状態をOpen(Vec<Entry>) / Closedに分け、終了済みの状態には
購読一覧を持てない形にした。closeはロック中にClosedへ置き換え、ロックを解放してから
逆順にGStreamerのsignal・probe・emit-statsを解除する。遅れて追加された登録も即時解除する。

登録・件数取得・終了にあったlock().unwrap()を除いた。
[Rust Mutexのpoisoning仕様](https://doc.rust-lang.org/std/sync/struct.Mutex.html#poisoning)
に従い、PoisonErrorからガードを取り出して解除可能な登録情報を保持する。
ここにはそれぞれ独立して解除できるEntryと終了状態だけがあり、複数Entryにまたがる
復元対象の不変条件はない。ClosedをOpenへ戻したり、データを捨てて登録を忘れたりしない。
この理由はコードにも記述した。メモリー割当・スレッド・継続タスクは追加しない。

これは任意のパニックから復帰する仕組みではない。特にGStreamerのFFI境界で発生した
パニックや、字幕パーサー内部の不整合を復元するものではない。
この段階で残っていたparser.lock().unwrap()への対応は次節を参照する。
SubtitleClockのロック失敗時の対応は「字幕時計のpoison検知」を参照。

CPU試験では意図的にpoisonした登録一覧を終了し、probeの保持オブジェクトの解放、
emit-stats停止、終了後に追加したsignal/probeの即時解放、複数回closeを検証する。
さらにコールバックの破棄時点で状態がClosedかつロックが解放済みであることを確認する。
GStreamerのPad・NULL状態のtsdemuxのみを使い、表示・音声・GPUは利用しない。

変更後のfeaturesモジュール試験は41件成功、外部TSを要求する1件は未実行。
字幕のdemux・時刻対応試験はファイルからメモリーへのCPU処理で検証した。
全ターゲットClippyは警告なし、fmt・diff検査も成功。
実再生検証はPulseAudio接続切断のため保留しており、この変更で音声接続の問題が
解決したことや、長時間のメモリー安定性を確認したことを意味しない。

## 字幕入力のpoison検知

ストリーミングスレッドでのBuffer/BufferList入力をingest.rsへ分離した。
パーサー・デコード件数・エラー通知を1つのArcで共有し、以前と同じく
1回のprobe呼び出しにつき1回ロックし、188バイト単位でパーサーへ渡す。

parser.lock().unwrap()を除き、poisonの場合は内部データを再利用せず入力を停止する。
通知には単一のAtomicBoolを使う。キュー・履歴・繰り返しログを追加しない。
他のデータの公開を伴わないフラグなのでRelaxed orderingとし、根拠をコードに記載した。
字幕時計を無効化して待機字幕を破棄し、Session::pollは型付きParserPoisonedを返す。
Playerは表示字幕を消し、subtitles_active=falseでQtの字幕pollタイマーを止め、
字幕ステータスへ再生し直す案内を表示する。保存された字幕有効設定は変更しない。
このステータスは設定画面の字幕項目へ接続し、PlainTextとして折り返し表示する。

映像・音声のパイプラインはここから操作しない。probeはそのまま保持し、失敗後の入力は
フラグ確認だけで返す。Removeを返すと購読管理に古いIDが残るため、READYにした後の
既存Session終了経路で解除する。異常なパーサーのメモリーもその世代の終了時に解放する。
停止・再生で新しいSessionを作るときにだけ状態を初期化する。

CPU試験ではパーサーのMutexを意図的にpoisonし、待機字幕の消去、継続入力の拒否、
Session::pollの型付きエラー、Session破棄、新しい世代での入力受付を確認する。
通常の字幕解析・時刻対応の既存試験も実行する。UIのエラー表示と実放送継続は未検証。
この変更は任意のパニックを捕捉する機構ではなく、FFI境界での最初のパニックによる
プロセス終了を防ぐものでもない。SubtitleClock自身のpoison検知・通知は次節を参照する。

検証は字幕CPU試験21件成功、外部TSが必要な1件は未実行。
全ターゲットClippy、fmt、SettingsDrawerのqmllintが成功した。
設定画面への接続を含むreleaseビルドも成功した。
音声出力の復旧確認がないため、実アプリの起動・描画・音声出力は試していない。

## 字幕時計のpoison検知

SubtitleClock::pollはロック失敗をUnchangedへ置換せず、ResultでClockPoisonedを返す。
時刻対応の状態と、各映像padのsegmentのMutexを区別し、後者の異常は共有フラグへ記録する。
どちらの異常も同じ再生世代の時計全体を無効とし、正常な新しいpadが現れても復帰させない。
フラグは既存のArc内へ置き、ヒープ割当・履歴・通知キューは追加しない。

状態参照は共通のResultを返すメソッドへ集約する。poisonの可能性を事前確認した後も、
実際のlockのResultを検査して競合中の異常を見逃さない。異常を検知した時計へは、
TSの統計・字幕キュー・時刻アンカーの更新を行わず、新しい映像padの購読も追加しない。
字幕入力は同じ時計の状態を調べて処理を止める。Session::pollから既存のPlayerの
エラー経路へ通知し、字幕表示とQtのpollを止める。映像・音声の制御には触れない。

異常な時計の内容を復元したり、poisonを解除したりはしない。保持中のキューは
その再生世代が終了してSessionとコールバックが破棄される際に解放される。
これはパニックそのものの捕捉・復旧機構ではない。

CPU試験で時計とsegmentをそれぞれ意図的にpoisonし、型付きエラー、入力停止、
有効化操作やpad変更でも復帰しないこと、新しい世代では正常に開始できることを確認する。
既存の時刻境界・demuxとのPTS対応試験は正常系のResultを検査して継続する。

字幕CPU試験23件成功、外部TSが必要な1件は未実行。全ターゲットClippyは警告なし、
fmt・diff検査とreleaseビルドも成功。実再生・エラーの画面表示・長時間の性能測定は未検証。

変更後の字幕関連CPU試験20件成功・外部TS依存1件未実行。Clippy全ターゲット、fmt、
releaseビルド成功。今回の変更後の実放送による字幕表示確認は未実施。


## 表示境界のエラー処理と分離

poll_subtitles/display_subtitlesをplayer/subtitle_rendering.rsへ集約した。
字幕JSON生成のResultを空文字へ置換していた処理を、成功時だけ表示とセル数を更新する
matchへ変更した。失敗はPresentationFailed(serde_json::Error)として保持し、詳細を一度
記録して表示・セル数を消し、字幕pollを停止する。解析・時刻異常も同じ消去経路を使う。
日英の案内は字幕状態の既存翻訳経路で再投影する。

現在のSubtitleCueは通常JSONへ変換できる構造であり、実放送でシリアライズ失敗を
再現したわけではない。Resultを返す境界の失敗を成功扱いしないための変更である。
失敗後のSession解放は既存の再生停止・終了経路で行い、表示失敗から再生を停止しない。

字幕CPU試験23件が成功し、外部TS指定が必要な1件は未実行。
QtCore/QMLの翻訳切り替え・カタログ欠落試験も成功（184項目）。
追加エラーの実GUI表示、実字幕・長時間メモリーの再検証は未実施。

全ターゲットClippy（警告をエラー扱い）、fmt・diff検査、CMakeリリースビルドも成功。


## 時刻到達済み字幕の選択

Timeline::pollは、到達済みの字幕をVecへ集めて安定ソートする代わりに、
最大の表示時刻を持つ1件をOptionへ保持する。同時刻は後から届いた字幕を優先し、
従来の安定ソートの最後の要素と同じ結果を選ぶ。途中の画面はUIへ通知されないため、
最新の字幕の表示期限・消去判定だけを適用する。未来の字幕は元の待機列へ戻す。

これにより到達済み字幕用の一時配列とソートを除去し、選択は待機件数nに対して
O(n)、追加の保持は1画面になる。字幕そのものの文字列・セルと待機列の割り当ては残る。
アプリ全体のRSSや実描画速度の改善量は測定していない。

追加のCPU試験で、UI更新が遅れた際の逆順到着、同時刻の消去優先／表示優先、
置換後の表示期限、未来の字幕の保持を確認した。字幕関連24件成功・外部TS依存1件未実行。
既存のPTS対応・wraparound・上限・リセット試験も含む。

Clippy全ターゲット・fmt・diff検査とCMakeリリースビルドも成功。実放送の再検証は残る。

## TS入力バッファの移動回数

TransportParser::pushは188バイトのパケットごとにVec先頭をdrainしていたため、
大きな入力では未処理部分を繰り返し移動していた。解析位置をusizeで進め、
処理済み範囲を呼び出し末尾で一度だけdrainする方式へ変更した。
同期マーカー検出、次パケットの同期確認、188バイト未満の末尾保持と
handle_packetへ渡す固定長コピーは維持する。新しい待機キューやキャッシュは追加しない。
実再生のCPU時間・RSS改善率は未測定であり、データ移動回数を減らす変更として扱う。
追加した回帰試験は生成PAT 256パケットと先頭の同期ずれを使い、入力サイズ
1/17/187/188/189/16384バイトと一括入力で同じPMT PIDを発見すること、
73バイトの未完了末尾が次回入力で完了して解放されることを検査する。
既存の字幕時計用TSはARIB字幕PID発見用ではないため、このフレーミング試験には
生成したテーブルを使用する。ARIB文字・配置・PTSの検証は既存の専用試験が担当する。
変更後のRust全体試験は95成功・3任意試験除外。全ターゲットClippyも警告なし。
起動中のアプリには入力を送らず、今回の変更での実放送再生はまだ行っていない。
CMake releaseビルドも成功した。

## 現行TSパーサーの実放送データ検証（2026-09-07）

Mirakurunの関西テレビ（HTTPサービスID 3272402080）から20.04秒、
39,436,288 bytesのTSを取得した。起動中アプリには操作を送らず、別の短いHTTP取得を
使用した。記録はgit管理外の`benchmark/live-subtitle-check/capture.ts`に保持する。

任意試験discovers_caption_stream_in_fixtureで放送serviceId=2080を明示して解析し、
ARIB字幕PIDの発見、字幕画面2件、配置済みセルを持つ画面1件、PTS付き画面2件を確認した。
試験は成功、解析試験自体の経過は0.13秒。この一回の数値を実再生のCPU改善率に換算しない。
映像との同期・Qt描画はこのCPU試験の範囲外。全放送・全字幕形式の互換性も証明しない。

検証用テストはResultで環境変数・ファイルIO・serviceId解析の失敗を返し、
固定16KiBの入力配列で逐次処理する。全TSと全字幕画面をVecへ蓄積せず、
画面・配置・PTSの件数だけを残す。serviceId未指定時の従来の探索も維持する。
指定パスはCargoのテスト実行ディレクトリーに依存しない絶対パスを使用する。

```sh
MIRAKURUN_SUBTITLE_TS_FIXTURE=/home/workshop/qt-gstreamer-features/benchmark/live-subtitle-check/capture.ts MIRAKURUN_SUBTITLE_SERVICE_ID=2080 CARGO_TARGET_DIR=build/cargo cargo test --locked --manifest-path rust/Cargo.toml discovers_caption_stream_in_fixture -- --ignored --nocapture
```

結果ログは同ディレクトリーのresult.log。本変更は検証用テストのみであり、
直前にビルドした実アプリのコードには変更を加えない。
全ターゲットClippyとfmt・diff検査も成功。
