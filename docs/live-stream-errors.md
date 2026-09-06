# ライブ配信の途中再開エラー（2026-09-06）

実視聴ログで `souphttpsrc` の `Server does not support seeking` を確認。
サービス3272202064で再生中、HTTP Rangeを受け付けないことを理由に停止した。
番組の切り替わりとの時間的関係は元ログだけでは確定できない。

GStreamer 1.28.2の[HTTPソース実装](https://github.com/GStreamer/gstreamer/blob/1.28.2/subprojects/gst-plugins-good/ext/soup/gstsouphttpsrc.c)
では、読み取りエラーの後に現在のバイト位置から要求を再発行する経路がある。
受信が進むとretryカウンターもリセットされるため、`retries=0` の指定だけでは
この再要求を防げなかった。is-liveの変更や字幕の無効化を解決策と決めつけない。

## 再現と対処

表示・GPU・音声を使わないHTTPソース→appsinkのテストで、chunked応答を途中で
切断すると同じエラーを再現。要求は通常GET→Range付きGETの順だった。

この検証版は、souphttpsrcが発行するResourceError::Seekの場合だけ、旧再生をREADYへ
停止し、字幕Sessionを解放して、新たなURI接続を1回開始する。再接続後に同じエラーが
再発した場合は停止・表示する。404など別のエラーには適用しない。利用者の再生操作
で復旧回数をリセットする。これは汎用の無制限自動再接続ではない。

実放送を中継するローカルHTTPプロキシで2回切断して検証したところ、要求は
通常GET→Range付きGET→新規GET→Range付きGETの4回。
最初の切断から再生復帰し、2回目の切断後は停止。5回目の接続は発生しなかった。
停止後は字幕購読・待機とも0、プロセスも正常終了。

自動テストは28件成功、外部TSを必要とする任意テスト1件は未実行。
ローカルの再現コード・要求記録・ログは `benchmark/stream-error/` に保存。
実際の放送局の番組切り替わりを待って再検証した結果ではない。
