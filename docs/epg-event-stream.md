# EPG変更通知の移植

mainの`rust/src/epg_events.rs`は`/api/events/stream?resource=program`を購読し、
番組のcreate/update/removeから更新要求を作る。通知を60秒間隔で集約し、切断時にも
取りこぼしを補う更新を要求する。実験版でもEPG有効時の購読と既存の再取得処理へ接続した。

[Mirakurun公式API定義](https://github.com/Chinachu/Mirakurun/blob/master/api.yml)で、
EventResourceのprogram/service/tunerとEventTypeのcreate/update/removeを確認した。
APIのクライアント実装もgetEventsStreamを提供している。
解析の互換対象はmainと同じ開いたJSON配列内のオブジェクト列であり、SSEのdata行ではない。

## 分離した解析・集約

`rust/crates/viewer-epg-events`の既定featureは通信・Qt・再生資源を持たない。
DecoderはBetween / Object / Failedで状態を表し、引用符・エスケープ・ネストをバイト単位で
追跡する。オブジェクトが完成するまでserdeによる解析を行わないため、細切れの大きな
イベントを毎回先頭からJSON解析する処理を避ける。完成時にresource/typeだけを読み、
番組本文や履歴を保存しない。文字列内の括弧とUTF-8の途中の分割を許容する。
配列区切りの扱いはmain同様に寛容で、配列全体の厳密なJSON妥当性判定器ではない。

1イベントは1MiBまで。正常時は一つのバッファを再利用するので、大きなイベント後は
その容量を接続終了まで保持する。内容は完成時に消去し、過去のイベントを蓄積しない。
解析失敗・上限超過はResultで返し、バッファを解放してFailedへ移る。再利用による
曖昧な復旧はせず、新しい接続は新しいDecoderを使う。
同じchunk内に正常イベントと不正データが混在するとErrになるため、受信側は切断時にも
必ず更新を要求する必要がある。

RefreshGateは変更ありのbool一つを保持し、60秒ごとに一回の更新要求へ集約する。
新しいイベントが来なくてもtake_dueを定期的に呼ぶ必要がある。
通知が集中しても履歴・要求キューを増やさない。時計は引数で注入し、試験で実時間を待たない。

## 検証と残作業

単独crateのテスト5件、Clippy全ターゲット（警告をエラー扱い）が成功。
全分割位置でのUTF-8・エスケープ・入れ子、対象イベントの絞り込み、正確に1MiBの
イベントを1バイトずつ供給するケース、1000イベント後のバッファ再利用、上限超過・
不正JSON後の解放と再利用拒否、1万通知の集約と静かな末尾の更新を確認した。

## 単独crateのHTTP購読

network featureにconnectionを追加した。Clientは総要求・readタイムアウトを設定しない専用型で、
接続フェーズは5秒、応答ヘッダー全体はTokioのtimeoutで10秒まで待つ。
[reqwest公式ドキュメント](https://docs.rs/reqwest/0.13.4/reqwest/struct.ClientBuilder.html#method.timeout)
で、総要求タイムアウトは本文終了まで適用され、既定は無制限であることを確認した。
有限JSON用Clientを誤って渡せない型にしている。quietなイベント本文は接続を維持する。

Subscriptionは呼び出し元のRuntime Handleで一つのタスクを起動する。受信はbytes_streamと
周期tickをselectし、後続イベントがなくても更新集約をflushする。変更通知はAtomicBool一つで、
受け手が遅くても通知キューを増やさない。接続状態はwatchで最新値だけ保持する。
HTTP・ヘッダー期限・JSON解析・EOFは型付きの失敗として保持し、HTTPエラーのURLは除去する。

EOF・受信エラー・不正データ・接続失敗後は更新要求を記録し、5秒後に再接続する。
RefreshGateは再接続を跨いで保持するため、失敗ループで更新要求の頻度を増やさず、
再接続後に新しい通知がなくても取りこぼし分を再取得できる。
ヘッダー待ちの間は集約のflushが遅れる可能性があり、厳密な60秒以内の更新保証ではない。

stopはSubscriptionを消費して停止専用Stoppingへ移る。旧通知を読むAPIはそこで失われ、
abort要求後にwaitでタスクの終了を確認する。waitはキャンセルを正常終了として扱い、
panic等のJoinErrorは返す。Dropはabortを要求するだけでjoinしないため、置き換える側は
Stoppingの完了を観測してから次の購読を開始する必要がある。

network有効のテスト8件と全ターゲットClippy（警告をエラー扱い）が成功。
追加のローカルHTTP試験3件は、本文がヘッダー期限を超えて静かでも接続維持すること、
通知なしの再接続でも更新を要求すること、ヘッダー期限切れ・明示キャンセル・本文受信中の
停止でソケットを解放することを確認した。時間条件は同じ実装に短いテスト値を注入している。
実サービスに対する10秒/60秒の待機・長時間検証を行ったものではない。

network無効でも既存の解析・集約テスト5件が成功。

## アプリとの接続

Controllerは希望する接続先とIdle / Running / Stoppingを保持する。
EPG有効かつ接続先を検証して局一覧取得を開始した場合だけ購読する。
局一覧が空でも購読を維持し、通知による局一覧の再取得を可能にする。
停止・再生・選局では購読を作り直さない。EPG無効化、サーバー変更、アプリ終了で停止する。

Stopping::try_finishはGUIのpollから非同期タスクの完了を非ブロッキングで確認する。
未完了なら所有権を保持し、完了後に最新の希望接続先だけを起動する。
旧購読の通知は停止開始時点で読めなくなる。専用HTTP Clientは有効時に遅延生成し、
無効化でControllerの参照を解放する。実行タスクの参照はタスク終了時に解放する。
アプリ終了時の最終的なタスク破棄は既存NetworkのRuntime終了にも依存する。

Playerは通知をEPG再取得と局一覧の強制更新へ渡す。EPG取得中の通知はbool一つに
集約し、現在の取得が完了したあと一度再取得する。接続先変更・無効化でこの要求も破棄する。
局一覧の取得中は重複要求を開始せず、次回の通知または既存5分更新で再取得する。
接続エラーは最新の失敗だけ保持し、同一失敗を毎poll出力しない。

Controllerの追加テストは、実行をyieldする前の連続した接続先変更で旧タスクの終了待ちを
保持することと、ローカルHTTPで有効・無効を10回繰り返して各本文ソケットのEOFを
確認する。EPGの既存テストも取得中の100通知から一回の追加取得になることを検証する。
実Mirakurunでのイベント反映、実GUIとの併用、長時間のRSS測定は未検証。

接続後の検証：network有効の単独crate 10件、network無効5件、アプリのEPG関連12件が成功。
単独crateとアプリの全ターゲットClippy（`-D warnings`）、rustfmt、差分の空白検査が成功。
`cmake --build build`によるリリースビルドと実行ファイルの配置も成功。
これらはCPUとローカルHTTPの試験であり、実再生のメモリー安定性を証明するものではない。


## Qt接続の責務分離

Player本体のEPG購読設定・通知の消費・番組取得・表示更新をplayer/epg.rsへ移動し、
10秒ごとの機能カウンター表示は既存player/telemetry.rsへ移動した。
poll_featuresは通知消費→実況→EPG取得と投影→カウンターの呼び出し順だけを表す。
EPGスナップショット、購読Controller、Runtimeの所有者は変えず、コピーや別タスクを増やさない。
番組表の日時選択と現在番組の投影は既存guide/program_infoモジュールを呼び出す。

分離時に、診断イベントをFetching状態の前後比較から推測すると、同じpoll内の取得完了と
追加取得開始が両方消える問題を確認した。ProgramInfo::pollはUpdateを返し、
completed: Option<Completion>とstarted: boolで両方を表す。CompletionはSucceeded/Failed。
Qt側は完了を先に、次の開始を後に記録する。固定サイズの結果で、通知キューを追加しない。
キャンセルした世代は成功・失敗として扱わない。

既存のローカルHTTP試験に、取得中の100通知による追加取得が「成功＋開始」を同時に返す
こと、通常完了と解析失敗も型で区別されることの検証を追加した。

EPG関連12試験と強化した遷移試験、全ターゲットClippy、fmt・diff検査、CMakeリリースビルドが成功。実サーバーでの診断ログ照合は未検証。


## 実サーバーへの短時間購読

2026-09-07、設定済みMirakurunの `/api/events/stream?resource=program` はHTTP 200、
Content-Type application/json; charset=utf-8、chunkedで応答した。
独立した15秒の読み取り観測では開始配列の2 bytesのみで、更新イベントは届かなかった。
観測用クライアントを閉じてから、本体と同じconnection::Client／Subscriptionを使う
任意試験を30秒実行した。Receivingを確認し、100msごとの状態観測でRetrying／WorkerStoppedを
検出せず、refresh通知は0件、stop().wait()は5秒の期限内に完了した。

```sh
MIRAKURUN_EVENT_URL='http://your-server:40772/api/events/stream?resource=program' \
CARGO_TARGET_DIR=build/epg-events cargo test --locked \
  --manifest-path rust/crates/viewer-epg-events/Cargo.toml --features network \
  real_server_subscription_stays_open_and_stops -- --ignored --nocapture
```

通常試験ではこの外部サーバー依存試験をignoredとする。試験中に異常を観測した場合も、
キャンセルの終了待ちをしてから失敗を返す。ネットワークのみを使用し、表示・GPU・音声を
使用しない。サーバーの番組情報や設定を書き換えてイベントを発生させることはしていない。

任意試験と同crateのnetwork有効・全ターゲットClippy、fmt・diff検査が成功。
今回確認できたのは静かな実接続の維持と停止まで。実更新イベントの内容、集約後のEPG再取得、
Qt画面への反映、長時間の資源推移は未検証。アプリ本体を変更しておらず再ビルドは不要。

## イベント集約からQt画面までの統合試験（2026-09-07）

85c85ccのアプリを専用Xvfb :99 / llvmpipeで起動し、実際の60秒ゲートを変更せず検証した。
専用設定でEPGのみ有効、再生停止中。検証用HTTPサーバーは1局・1番組とopen-array形式の
イベント接続を提供する。実Mirakurunへの書き込みや実放送データの変更はしていない。

初回取得した「EPG A」を番組表で表示した後、サーバー側の番組名を「EPG B」へ変更し、
program/update通知を1万件送った。イベント接続はその後も改行を送り、切断しない。
最初のprograms取得から60.004秒後に追加取得が1回だけ発生し、servicesも1回再取得。
番組表を閉じずにカードがEPG Bへ更新され、ウィンドウタイトルもEPG Bになった。
初回取得から125秒まで観測し、programs要求は合計2回、イベント接続は1本のまま。
次の1分間に再取得はなく、5分の定期取得や切断後の再同期との混同もない。

診断ログもepg_fetch_started→epg_fetch_finishedが2組。初回はelapsed 234→284ms、
更新時は60237→60287msで、更新時のguide_open=true・loading true→falseと一致した。
保持番組数は1件、診断記録の欠落数は0。これはイベントの相関確認で、性能測定ではない。
QML例外・イベント接続エラーなし。アプリは終了コード0、サーバー側でもイベント接続の
切断を観測してから検証サーバーを終了した。

証跡はGit対象外の `benchmark/epg-event-ui/`：server.py・server.log・player.log、
before.png・after.png、diagnostic-events.jsonと元の診断JSONL。
イベント受信→集約→再取得→Qt反映は検証用データで確認済みとなる。
実Mirakurunが発する更新通知、実再生との同時実行、長時間の資源推移は引き続き未検証。
