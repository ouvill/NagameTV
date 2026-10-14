# 選局と視聴中のpoll処理の計測

製品のMain.qml、映像パイプライン、音声出力を使い、選局要求から最初の映像フレームまでを
測る任意の診断スイート。表示・実GPU・音声が必要で、公開ランナーが専用GUIセッションを
検証してから実行する。共通ビルドロックは計測終了まで保持する。

```sh
CARGO_TARGET_DIR=build/cargo python3 scripts/test.py startup --profile release -- playback-performance
```

試験用サーバーは127.0.0.1にだけ接続する。既存のタイムシフト試験と同じ
`tests/fixtures/recording-seek.ts`（160×90、MPEG-2、25 fps、無音AAC）を20 ms間隔で配信する。
同じTSを返す2局を交互に7回選び、選局ごとの通信・再生資源の入れ替えを測る。
実チューナーの選局時間、放送波のビットレート・GOP構造・通信の揺らぎは再現しない。

結果は共通ランナーの`build/test-runs/run-*/`のstartupログへ出力する。
`PLAYBACK_PERF`行のJSONに各試行、中央値、最小値、最大値を記録する。

| 項目 | 測定範囲 |
| --- | --- |
| `request_ms` | QMLから選局・再生を要求し、同期呼び出しが戻るまで |
| `playing_ms` | 要求からPlayerのPLAYING状態を観測するまで |
| `first_video_ms` | 要求から映像sinkのrendered > 0を観測するまで（PLAYING通知とは独立に観測） |
| `first_presented_video_ms` | 要求から、新しいストリームの映像をQtが提示したことを受信遅延の最初のサンプルで観測するまで。prerollの提示も含む |
| `idle_poll_us` | 未再生の`player.poll()`呼び出し。100回ずつ7組の平均所要時間（µs/回） |
| `live_controls_visible_poll_us` / `live_controls_hidden_poll_us` | 操作部を表示 / 非表示に固定した再生中のpoll。同じ100回×7組で、自動非表示による条件の混在を防ぐ |
| `final_video_stats` | 最後の試行後の動画統計。途中の選局でカウンターがリセットされるため、全試行の累計ではない |

状態は5 ms間隔で観測する。JSON取得・Qtイベント処理の時間も含むため、フレーム到着の
精密なタイムスタンプではない。`first_video_ms`は過去の結果と比較するためのsink指標で、
Qtの初回映像提示時刻ではない。[GstVideoSink](https://gstreamer.freedesktop.org/documentation/video/gstvideosink.html#GstVideoSink:show-preroll-frame)
はpreroll中にも映像を提示できるが、[GstBaseSinkのrenderedカウンター](https://github.com/GStreamer/gstreamer/blob/1.28.2/subprojects/gstreamer/libs/gst/base/gstbasesink.c#L3982)
は通常のrender後に加算され、prerollの提示では増えない。
そのため、renderedを待つ時間を黒画面の継続時間として扱わない。

`first_presented_video_ms`は既存のスクリーンショットと同じGstVideoMetaのマップ監視を通し、
QtのframeSwapped後に確定した`receive_latency`サンプルを使う。受信PTSと照合できた
フレームが対象で、照合できない先頭フレームがあると後続フレームの観測になる。選局時に
前のストリームのサンプルを破棄するため、前の局の表示を数えない。UIによる遮蔽や物理ディスプレイ上の
表示を保証するものではない。音声の初回出力時刻もこのスイートでは測らない。

`PLAYBACK_LOAD_MS`は再生基盤のpreloadとQMLエンジン生成・Main.qmlロードの時間。
Qtアプリケーション初期化後からの計測であり、プロセス開始からウィンドウ表示までの
時間とは区別する。pollの連続呼び出しも通常の50 msタイマーとは異なるため、CPU使用率へ
直接換算せず、同条件の前後比較に使う。

定常CPU・RSS・破棄フレームの比較には
[既存のメモリー計測](playback-memory-architecture.md#計測方法)、
番組表の生成・スクロールには[番組表の計測](guide-performance.md)を併用する。
性能値に固定の合否閾値は設けない。再生失敗や初回フレームのタイムアウトは失敗として扱う。
