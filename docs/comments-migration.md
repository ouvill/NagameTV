# 実況機能の移植

移植元はmainのrust/src/comments.rs、viewer-core/src/comments.rsとapp.rs、
Main.qmlのdanmakuLayer・実況一覧・設定。目標は受信だけでなく、接続の世代管理、
チャンネル対応、再接続、履歴一覧、画面上の実況、各設定とmainの表示デザインを再現すること。
実験アプリには受信・チャンネル追随・履歴一覧・機能の有効／無効を組み込んだ。
画面上を流れる実況と表示調整、mainとの実画面比較は残っている。
以下は段階ごとの実装記録で、現在の組み込み状況は末尾を参照。

## プロトコルの分離

rust/crates/viewer-commentsはQt・GStreamer・デバイス・非同期実行環境に依存しない。
通信の解析・購読リクエストの生成を先に切り出した。今後の受信タスクはこのcrateを利用し、
その結果をアプリの接続状態とQtの表示モデルへ渡す。
単独crateにすることで、再生環境を使わず通信データの契約を検証できる。
後述のnetwork featureを有効にすると非同期受信処理も利用できる。

[NX-Jikkyo公式リポジトリー](https://github.com/tsukumijima/NX-Jikkyo)に記載された
スレッド一覧とコメント用WebSocketを対象とし、mainの購読リクエストを維持する。
thread idはJSONの文字列として送信し、最後の100件とその後の新着を受信する。
IDの型をThreadIdに分け、u64をQtの数値へ変換する経路は持たない。

Decoderは接続ごとにHistoryから開始し、main同様にpingのrf:0でLiveへ移る。
接続を作り直す場合はDecoderも新しくする。解析結果はIgnored / HistoryComplete /
Commentで区別する。Commentは本文・送信元区分・履歴／新着・時刻だけを所有し、
ユーザーIDやその他のwireメタデータ、過去に読んだJSONを保持しない。
本文は表示層でPlainTextとして扱う必要がある。HTMLを削除・解釈して別の文字列にはしない。

上限はJSONメッセージ64KiB、デコード後の本文4096バイト、スレッド一覧1MiB。
空または長すぎる本文はIgnored、不正JSON・型・一覧にACTIVEなしは型付きErrorとする。
wire文字列にはCowを使用し、JSONエスケープがない場合の一時コピーを避ける。
一覧は上限付きbodyから必要なid/statusのみを取り出し、呼び出し終了後に解放する。
ネットワーク側でも受信中のbody/frameサイズ上限を設ける必要があり、解析前のサイズ検査だけで
通信バッファのメモリー上限を保証するものではない。

mainの日本時間表示を維持しつつ、dateを一日分に剰余化してから9時間を足す。
u64::MAXでもオーバーフローせず16:00:15になる。元のタイムスタンプは変更しない。
欠落したdateはmainと同じ0を使う。不正な型・負数・u64を超える値はJSONエラーとする。

## 検証と残作業

単独crateのCPUテスト5件、Clippy全ターゲット、fmtが成功。
履歴から新着への切り替え・接続し直したときの初期状態・JSON途中切れ・入力上限・
UTF-8とエスケープ後の本文長・大きいIDと時刻・送信元と文字列保持を検証した。
外部サービスへの接続やメッセージ投稿は実行していない。

次に行うのはmainの放送局対応表、同時に1つだけの受信タスク、旧接続からの結果の拒否、
有限キュー、タイムアウトと終了、接続状態の通知の移植。その後、Qt側の履歴と描画を接続する。
mainとの実画面比較、実サービス受信、長時間のメモリー・CPU・フレーム時間測定は未実施。

検証コマンド:

```sh
CARGO_TARGET_DIR=build/comments-protocol cargo test --locked --manifest-path rust/crates/viewer-comments/Cargo.toml
CARGO_TARGET_DIR=build/comments-protocol cargo clippy --locked --manifest-path rust/crates/viewer-comments/Cargo.toml --all-targets -- -D warnings
```

## 接続世代ごとの受信タスク

network featureにConnectionを追加した。アプリの既存Tokio Handleとreqwest Clientを
受け取り、独自のruntime・スレッドは生成しない。1つのConnectionは1つの受信タスク、
専用のコメントキュー、最新状態のwatch通知を所有する。Qtや再生デバイスへの依存はない。
HTTPとWebSocketのURLはアダプターから渡し、今回の試験はloopbackのみへ接続した。

コメントキューはmainと同じ256件でtry_sendし、満杯なら新着を破棄して件数を数える。
本体は最大4096バイトなので、キューに保持する本文は最大1MiB。これは文字列以外の
メタデータ・通信バッファ・TLS・allocator等を含むプロセスRSSの上限ではない。
Connecting / Receiving / Ended / Failedは別のwatch経路で最新値1つだけを保持する。
コメント過多でも終了通知を落とさず、予期せぬタスク終了もエラーとして取得できる。

HTTPはContent-Lengthと読み取り中の実サイズの両方で1MiBを制限する。
ヘッダーだけでなくbody読み取り全体に10秒、WebSocket接続・購読送信・制御応答にも
それぞれ10秒の期限を設ける。接続後のコメント待機に周期Timerは使わない。
コメントがないだけでは切断せず、socket.nextの待機もタスクのabortで中断できる。

mainと同じtokio-tungstenite 0.28系を使い、frame/messageを64KiB、read bufferを8KiB、
write bufferの上限を64KiBとする。購読リクエスト以外にアプリのコメントは送信しない。
[Tungsteniteの制御応答](https://docs.rs/tungstenite/0.28.0/tungstenite/protocol/struct.WebSocket.html)
に合わせ、PingとCloseでは自動応答をflushする。ローカルpeerでもPongを確認した。
JSONとして壊れた個別コメントはmain同様に読み飛ばす。transportのサイズ違反は終了エラーにする。

Connection::stopは自身を消費し、その世代のキューを直ちに破棄してStoppingを返す。
Stopping::waitまたはis_finishedで終了を確認してから後続を作る必要がある。
[Tokio JoinHandleの仕様](https://docs.rs/tokio/latest/tokio/task/struct.JoinHandle.html#method.abort)
ではabort呼び出し直後の終了は保証されないため、この確認を省略しない。
Dropもabortを要求するが、待機完了の保証はない。キャンセル時はclose handshakeを待たず
ソケットを破棄し、通常のサーバーCloseでは応答する。

network有効のテスト10件とClippy全ターゲットが成功。ローカルHTTP/WebSocketで
購読ID、履歴／新着、300件受信時の256件保持・44件破棄、終了状態の保持、Ping応答、
HTTP宣言サイズ超過、WebSocket frame上限、HTTPヘッダー待機中と無通信socketの
キャンセル後の接続解放を確認した。TLS・外部NX-Jikkyo・10秒期限の満了は未検証。

この時点ではアプリ本体からConnectionをまだ作成していない。アプリでの「同時に1接続」の
保証、放送局対応、切り替え時のStopping管理、自動再接続、設定、Qt表示と履歴の移植は残る。

```sh
CARGO_TARGET_DIR=build/comments-protocol cargo test --locked --manifest-path rust/crates/viewer-comments/Cargo.toml --features network
CARGO_TARGET_DIR=build/comments-protocol cargo clippy --locked --manifest-path rust/crates/viewer-comments/Cargo.toml --all-targets --features network -- -D warnings
```

## 接続切り替えと再接続

Controllerを追加し、Idle / Running / Stopping / Waitingのenumで接続の所有権を管理する。
接続先の変更は旧Connectionをstopしてキューを捨て、Stopping::is_finishedの確認後に
新しいタスクを生成する。停止中の変更はdesiredを上書きするだけで、要求一覧や待機タスクは
追加しない。同じ接続先を繰り返し指定しても接続や待機期限をやり直さない。

通常切断・エラーの後は5秒待って同じ接続先へ再接続する。これは受信モジュールの
明示的な再試行方針で、mainに同じ5秒タイマーがあったという意味ではない。
再試行の判断には呼び出し側から渡されたInstantを使い、GUI側に新しいタイマーやsleepは
持たない。無効化はdesired=Noneとし、終了待ち中も後続を作らない。
Disabledという表示状態だけで終了完了と判断せず、is_stoppedで停止完了を確認できる。

1回のpollが返すコメントは最大64件。空のpollではコメント用Vecのヒープ割当を行わない。
通常切断時は最後のキューを渡し終えてから停止・再試行へ移る。終了状態を先に観測してから
キューを読み出すことで、空キューの確認と終了通知の間に最後のコメントが届く競合でも
そのコメントを破棄しない。選局・無効化による明示的な変更では旧キューを即時破棄する。

Controller内に表示履歴はない。アプリ側は選択した放送サービスの変更時に表示履歴を消す
必要がある（複数の放送サービスが同じ実況URLに対応する場合も含む）。
Controller自身をDropすると現在の接続をキャンセルする。アプリ終了の待機方法は
今後のアダプター側で既存ネットワークruntimeの終了処理と合わせて実装する。

ローカル通信を使い、A→B→Cの連続変更でBを接続しないこと、終了前のpollでCを開始しない
こと、無効化後の停止完了、再試行の期限直前／期限到達、同じ接続先の再指定、
通常切断直前の130件を64・64・2件で失わず配送することを検証した。
network有効の試験13件と全ターゲットClippyが成功。放送局対応、アプリ本体への接続、
Qtの履歴・描画と設定、実NX-Jikkyo受信、長時間測定はまだ残る。


## アプリへの接続と履歴一覧

features/commentsが選局情報から実況先を選び、Controllerと履歴を所有する。
mainの地上波7局の名前対応を維持し、衛星は明示された放送serviceIdを使う。
102は101へ対応させる。Mirakurunのendpoint IDを割り算して推定しない。
同じ実況先でも放送サービスが変われば履歴を解放する。サーバー変更でも接続を停止する。
既存Networkのruntime・clientと既存pollを利用し、実況専用スレッドやGUI Timerを追加しない。

履歴は最新200件。無効化・選局変更ではVecDeque自体を作り直して保持領域を解放する。
パネルが開いていて履歴が変更された場合だけQt用JSONを生成し、閉じるとQt側は空配列にする。
現段階では更新ごとに最大200件のJSONを置き換える方式であり、差分モデルではない。
[Qt ListViewの公式仕様](https://doc.qt.io/qt-6/qml-qtquick-listview.html)に基づき、
必要な表示行を生成するListViewを使う。本文はPlainTextで表示する。
大量受信時の描画負荷は実測前で、200件上限だけでは性能検証の代わりにならない。

設定comments_enabledは既定false。設定画面の実況機能で変更・保存できる。
`--features=comments`および他機能とのカンマ区切りを受け付け、指定に含まれない実況は
設定画面から有効にできない。CLI実験起動は従来どおり永続設定を書き換えない。
mainのdanmaku_enabled等は既存の追加設定として保持し、履歴機能の許可と混同しない。
終了時は接続をabortし、Player破棄時の既存Network runtime破棄がタスク終了を待つ。

放送メタデータの対応、非対応局、大きいendpoint ID、1000件入力後の最新200件保持、
同じ実況先への別放送サービスの切り替え、無効化と保持領域解放をCPUテストで検証する。
外部NX-Jikkyo受信・実画面・長時間測定は未実施。流れるコメント、表示調整とmainの
実況パネル全体のデザイン再現は引き続き残作業。


組み込み時の検証結果: 放送対応・履歴2件、CLI指定1件、設定保存互換性5件、
単独受信crateのローカル通信を含む13件が成功。全ターゲットClippy、fmt、
CommentList / ProgramSidebar / SettingsDrawerのqmllint、CMakeリリースビルドも成功。
音声デバイス接続の問題が未解消のため、この変更で実アプリの再生・GUI試験は実行していない。
