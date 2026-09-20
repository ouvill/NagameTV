# デインターレース設定と動画統計の移植

## 録画の滑らかさの調査（2026-09-16）

2026-09-20追記: 継続再生中に時々小さな引っ掛かりがあり、sinkの破棄も少数あるとの
報告を受けた。ビルドなど重い処理の実行中にはフレーム落ちが増えるとの報告もある。
GPU経路導入時に4へ減らした映像キュー上限を8フレームへ戻し、
短い上流処理の遅れを吸収する余裕を増やした。変更後に少し安定したように感じるとの
反応があったが、原因の特定と改善の定量的な確認はできていない。
比較では開始・シーク直後を除き、通常再生中の破棄の増分と引っ掛かりの頻度を見る。
統計は1秒更新のため、表示値だけで瞬間的なキュー枯渇の有無は判定できない。
再生関連Rust試験81件とCMakeリリースビルドは成功。外部録画が必要な任意試験1件は未実施。

2026-09-16追記: 利用者から、一時的な負荷だった可能性があるため対策を当面見送るとの指示。
以下の測定記録は保持し、再発の報告があれば調査を再開する。

`benchmark/2026年09月13日19時00分00秒-授業科目案内「物理の世界('24) 身近な統計('24)」.m2ts`
を調べた。拡張子はm2tsだが、内容は188バイトのTS。長さは約912.741秒で、
映像はMPEG-2・1440×1080・29.970fps・mixed、PARは4:3、音声はAAC 48kHzステレオだった。

X11表示、NVIDIA RTX 4070 TiのOpenGL、PulseAudioの接続と出力先を検証し、
製品のMain.qmlと通常の再生パイプラインで冒頭・5分へのシーク後を各25秒測定した。
Waylandもソケットへの接続・roundtripを確認してから同じ素材の冒頭を測定した。
音声出力は実際のPulseAudio sinkを使用し、測定時はミュートした。

- 既定のYADIFは両フィールドを処理し、出力は59.940fpsのprogressive。
  安定再生中のsinkカウンターは毎秒約60増え、droppedは0、キューは概ね8フレームだった。
- CPUのみの切り分けでは、同じ素材の約20秒分を約5秒でデコード・YADIF処理した。
  出力PTSは1件の逆行（約16.683ms）を除き、約16.683msの間隔だった。
- QtのframeSwapped通知は普段16〜18ms前後だが、70〜96ms空く箇所もあった。
  再生位置・番組情報・字幕のUI定期更新を止めた比較でも70〜90msの間隔が残った。
  Waylandでの測定にも84msの間隔があったため、今回のシークUIや定期更新だけを
  原因とは断定できない。

**滑らかさの差の原因は未特定。** VLCはこのコンテナにないため、同じ環境での比較は未実施。
測定はnative_testsの開発ビルドに一時的な計測を入れて行い、Qtイベント処理を1ms間隔で
進めた。frameSwappedは同じ映像の再描画も含み、通知間隔にはGUIへの配送遅延も含まれる。
sinkのdroppedが0でも、ディスプレイへの全フレームの提示を保証しない。
描画・表示環境を含む原因の特定と、差が目立つ場面でのVLCとの比較は残っている。
根拠なく画質や同期の設定は変更していない。
一時的な計測コードは製品に含めず、ログと再現用コードはGit対象外の
`benchmark/recording-cadence/` に保存した。

## 実装

mainのplayback.rsとvideo_stats.rsの契約を移植し、処理別に分割した。

- `playback/deinterlace.rs`: 起動設定をMode enumへ検証し、映像処理elementを生成。
- `playback/video_output.rs`: sinkの形式を検証し、CPU／NVDEC／VAのメモリー制約を持つ出力binを構築。
- `playback/stats.rs`: 現在のcaps・sink統計・queue使用量を読み取り、所有するスナップショットを返す。
- `player/statistics.rs`: Qt境界でJSONへ変換。変換失敗はログへ記録する。
- `qml/VideoStats.qml`: 1秒ごとに値を更新するパネル。固定の11行を再利用し、更新ごとにdelegateを作り直さない。

## デインターレース

`NAGAMETV_DEINTERLACE=yadif|linear|off|gl|va`を起動時に指定する。既定はyadif。
GPU経路、NV12表示、デコーダー／メモリー形式の統計は[GPU映像処理](gpu-video.md)を参照。
mainと互換のquality・balanced・disabled、前後空白と大文字にも対応する。
不正な値は型付きエラーとして起動を失敗させる。実行中には変更しない。
Offはidentityを使い、YADIF/Linearはauto・all fieldsで処理する。

[公式deinterlace資料](https://gstreamer.freedesktop.org/documentation/deinterlace/index.html)では、
autoは入力のinterlace情報に応じた処理、all fieldsは両フィールドの出力となる。
これは入力が常にインターレース、または常に出力fpsが倍になるという保証ではない。

## 統計とメモリー

パネルのLoaderを有効にした時だけTimerを生成する。閉じるとLoaderがパネル・Timer・
最後のスナップショットを破棄する。Rustには統計用Timerや履歴、frameの参照を持たせない。
[Qt Loader](https://doc.qt.io/qt-6/qml-qtquick-loader.html#active-prop)に基づく。

入力・出力解像度、fps、走査方式、PAR、画素形式、deinterlace設定、sinkのrendered/dropped/
average-rate、queueのbuffer数・byte数・時間、GStreamerバージョンを表示する。
READY/NULLでは以前のcapsやフレーム数を表示しない。未知値・非有限値は「—」とする。

sinkは[GstBaseSink stats](https://gstreamer.freedesktop.org/documentation/base/gstbasesink.html#GstBaseSink:stats)、
queueは[current-levelプロパティ](https://gstreamer.freedesktop.org/documentation/coreelements/queue.html)を読む。
sinkの集計は画面への実表示回数ではなく、queue時間は放送からの遅延ではない。
この環境ではsink average-rateがcapsのfpsと大きく異なる小さな値になる場合があり、
実際の表示fpsとして解釈しない。ネイティブが返した値を診断情報として表示している。

## 検証

CPU-onlyの生成映像を使い、全3モードでフレーム処理とEOSを確認。
interleaved 30fps入力でYADIF/Linearは60fps、Offは30fpsの出力capsを確認した。
分数fpsの精度・可変fpsの未知値、READYへ停止後のcapsとカウンターの破棄も検証した。
テスト用pipelineもRAIIでNULLへ戻し、途中失敗時にストリーミングタスクを残さない。

自動テスト36件成功、外部TSが必要な任意テスト1件は未実行。
Clippy全ターゲット警告なし、ビルド成功。

実機試験はdisplay・NVIDIA OpenGL・PulseAudioの動作を確認した環境で行う。
証跡はGit対象外の `benchmark/video-stats-migration/` に保存。
最初のOff試験は20秒以内に自動再生を確認できず、正常終了した。再試験ではOffも
PLAYINGへ到達したが、最初の待ち時間超過の原因は未特定で、起動時間の保証はしない。
画面座標による停止操作が一部で成立しなかったため、追加試験ではテストプロセスの
Qt倍率を1に固定し、ウィンドウをアクティブにしてから座標を取得した。
それでも一部の停止後画像はPlayingのままであり、この自動クリックを停止成功とは数えない。
実機での停止後表示の検証は未完了。停止時のカウンター無効化はCPUテストで確認済み。
通常起動の倍率設定は変更していない。参考: [Qt High DPI](https://doc.qt.io/qt-6/highdpi.html)。

長時間のパネル開閉と資源測定、字幕・EPG等との全機能併用試験は今後の検証対象。

再試験ではOff/YADIF/LinearすべてでPLAYINGと統計の表示を確認し、正常終了した。
実放送のcapsは1440×1080、入力29.970fps、YADIF/Linear出力59.940fps、Off出力29.970fps。
不正な設定での起動は終了コード1とエラー表示を確認した。
統計の固定行は新しいsnapshotで値が更新されることを画像で確認した。

最終証跡は `recheck/`、`final/`、`xtest/` と対応するスクリプト・結果ログ。
画像名のstoppedは試験の操作名であり、上記の理由から状態の証明には使わない。


## mainの統計パネルとの表示差の補完

mainの表示領域寸法／DPR行と閉じるボタンを追加し、左16px・最大幅510px・
操作部の表示に追随する縦位置、背景・枠・角丸・余白・文字サイズを移植した。
項目名と説明はmainの既存翻訳カタログへ接続し、平均レートのfps単位も補完した。
カード内のクリックは背後の映像操作へ渡さず、閉じるボタンは通常の子コントロールとして扱う。

寸法は映像Itemのwidth/height、DPRは同ItemのScreen.devicePixelRatioへのバインディング。
[Qt Screen](https://doc.qt.io/qt-6/qml-qtquick-screen.html)が定義する物理ピクセルと
デバイス非依存ピクセルの比率を表示し、起動時の値を固定保存しない。
番組表を開いている間はLoaderを無効化し、統計のタイマーとスナップショットを解放する。
閉じる操作でも同じ解放経路を通る。毎秒の計測で行delegateを再生成しない構成を維持する。

VideoStatsのqmllint、187項目の翻訳切り替え／カタログ欠落試験、diff検査が成功。
実画面の重なり、クリック伝播、複数画面間のDPR追随は未検証。

QML事前コンパイルを含むCMakeリリースビルドも成功した。

## 仮想画面での描画領域表示の修正（2026-09-07）

ユーザー指定のXvfb :99 / llvmpipeでUI操作を検証した。GPU性能・RSS測定とは分ける。
実放送再生中に描画領域が `NaN × NaN / 0` となり、Main.qmlのDPR取得で
TypeErrorが発生した。VideoStatsの `video()` 関数が、Loader越しに参照する
親画面の `video` 要素名を隠していたため、整形関数を `formatVideo()` に変更した。
[Qtのスコープと名前解決](https://doc.qt.io/qt-6/qtqml-documents-scope.html)を参照。

Mainと同じunbound Component / Loader構成の回帰テストで、修正前はNaNと
TypeErrorを再現し、修正後は寸法・DPRとリサイズ追随が成功した。
閉じるアイコンのURLはテストから実ファイルを指定可能とし、製品の既定値は維持した。
QML全77件成功、CMakeリリースビルド成功。

実アプリで日本語・英語の統計表示、1440×900から900×560への寸法追随（DPR=1）、
停止後Ready・caps未取得・キュー0を確認し、正常終了した。修正後ログに上記の例外なし。
証跡はGit対象外の `benchmark/virtual-ui/stats-viewport-fixed.log`、
`stats-viewport-fixed.png`、`stats-audio-en-fixed.png`、`stats-viewport-resized.png`、
`stats-stopped-confirmed.png`、`stats-qml-results.txt`。
実放送の音声一覧で日本語の主音声とEnglishの副音声、および英語UIのラベルを確認。
音量0での操作なので、実際の二か国語音声の聴取確認には数えない。
異なるDPRの画面間移動、長時間の資源測定は引き続き未検証。

## 小さい画面での再生操作との重なり（2026-09-07）

900×560で統計を表示すると、パネルが下部の再生操作に重なった。
mainも画面下端のみを基準にする同じ配置式だったため、これは継承した問題の改善。
表示中の操作部の実際のy座標を統計の下端制約に使い、16pxの間隔を確保した。
通常の1440×900では従来どおりy=138。小さい画面では統計を上へ寄せる。
[Loaderのサイズ規則](https://doc.qt.io/qt-6/qml-qtquick-loader.html#loader-sizing-behavior)
に従い、明示しない高さは読み込んだ統計の内容に追随する。新しいタイマーや
スナップショットは追加しない。

CMakeリリースビルド成功。Xvfb :99 / llvmpipeで実放送を再生し、英語UI・900×560で
統計を開いたまま下部ボタンによる停止（Ready、キュー0）と再開（Playing、caps復帰）、
統計の閉じる操作を確認した。1440×900の配置も確認し、アプリは終了コード0。
証跡は `benchmark/virtual-ui/stats-controls-layout.log` と `stats-layout-*.png`。
この試験はUI操作のみであり、GPU性能・メモリー安定性の検証には数えない。
