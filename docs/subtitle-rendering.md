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
代替するものではない。字幕parserと購読管理のMutex処理の監査は引き続き残る。

変更後の字幕関連CPU試験20件成功・外部TS依存1件未実行。Clippy全ターゲット、fmt、
releaseビルド成功。今回の変更後の実放送による字幕表示確認は未実施。
