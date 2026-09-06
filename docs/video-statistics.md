# デインターレース設定と動画統計の移植

mainのplayback.rsとvideo_stats.rsの契約を移植し、処理別に分割した。

- `playback/deinterlace.rs`: 起動設定をMode enumへ検証し、映像処理elementを生成。
- `playback/stats.rs`: 現在のcaps・sink統計・queue使用量を読み取り、所有するスナップショットを返す。
- `player/statistics.rs`: Qt境界でJSONへ変換。変換失敗はログへ記録する。
- `qml/VideoStats.qml`: 1秒ごとに値を更新するパネル。固定の11行を再利用し、更新ごとにdelegateを作り直さない。

## デインターレース

`MIRAKURUN_DEINTERLACE=yadif|linear|off`を起動時に指定する。既定はyadif。
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
