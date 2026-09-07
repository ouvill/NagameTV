# 実況機能の移植

移植元はmainのrust/src/comments.rs、viewer-core/src/comments.rsとapp.rs、
Main.qmlのdanmakuLayer・実況一覧・設定。目標は受信だけでなく、接続の世代管理、
チャンネル対応、再接続、履歴一覧、画面上の実況、各設定とmainの表示デザインを再現すること。
実験アプリには受信・チャンネル追随・履歴一覧・機能の有効／無効を組み込んだ。
画面上を流れる実況と表示調整も組み込んだ。mainとの実画面比較は残っている。
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
mainの地上波7局の名前対応を維持し、地上波以外は明示された放送serviceIdを使う。
102は101へ対応させる。Mirakurunのendpoint IDを割り算して推定しない。
同じ実況先でも放送サービスが変われば履歴を解放する。サーバー変更でも接続を停止する。
既存Networkのruntime・clientと既存pollを利用し、実況専用スレッドやGUI Timerを追加しない。

履歴は最新200件。無効化・選局変更ではVecDeque自体を作り直して保持領域を解放する。
パネルが開いていて履歴が変更された場合だけQt用JSONを生成し、閉じるとQt側は空配列にする。
現段階では更新ごとに最大200件のJSONを置き換える方式であり、差分モデルではない。
[Qt ListViewの公式仕様](https://doc.qt.io/qt-6/qml-qtquick-listview.html)に基づき、
必要な表示行を生成するListViewを使う。本文はPlainTextで表示する。
大量受信時の描画負荷は実測前で、200件上限だけでは性能検証の代わりにならない。

設定comments_enabledは既定true（初期移植時のfalseからmain互換に修正）。
流れる表示のdanmaku_enabledはmainと同じ既定false。設定画面でそれぞれ変更・保存できる。
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


## 流れる実況と表示設定

mainのdanmakuLayerをDanmakuOverlay.qmlに切り出した。8段の配置、追い越しを考慮した
段選択、全段が埋まった際の最も先へ進んだ段の利用、9秒／速度倍率を基準とする
移動時間、太字・縁取り・番組名領域の回避を維持する。全段が混雑した場合にはmain同様に
文字が重なる可能性があり、常に衝突しない保証はない。同時表示はmainと同じ最大64件。
作成済みComponentからのみLabelを生成し、受信本文はPlainTextに渡す。

Rust側はPhase::LiveだけをcommentReceivedシグナルに投影する。履歴は一覧には残すが
画面に流さない。停止・描画無効時には新着を描画用QStringへ複製しない。1回のpollから
シグナルへ渡す一時配列は既存の受信上限64件以内で、そのpoll終了時に解放される。
描画のLoaderは再生中かつ実況機能・画面表示の両方が有効な場合だけ生成する。
停止・画面表示オフ・機能オフ・終了時にLoaderの所有オブジェクトを破棄する。
選局・サーバー変更では表示中の一覧と段参照を消し、動的生成したLabelを破棄する。

[Qtの動的オブジェクト仕様](https://doc.qt.io/qt-6/qtqml-javascript-dynamicobjectcreation.html)
に従い、createObjectで生成したLabelだけにdestroyを呼び出す。
Loaderが生成したルートの破棄はLoaderに任せる。destroyは遅延されるため、配列の参照を
先に外す。アニメーション完了時と明示クリアの両方から呼ばれてもdisposeは一度だけ行う。

mainのdanmaku_enabled・comment_font_size・comment_opacity・comment_speedを型付き
設定へ移行した。既存TOMLの同名値を読み書きし、範囲はmain同様にサイズ12〜48、
透明度0.1〜1、速度0.5〜2。永続値が非有限なら既定値、UI要求の非有限値は要求全体を拒否する。
スライダーの範囲はmainの14〜36／0.2〜1／0.5〜2を維持する。
設定画面を閉じる際と終了時に保存し、実況パネルの表示トグルは操作時に保存する。
画面表示をオフにしても受信と履歴一覧は継続し、実況機能自体をオフにすると受信も停止する。

CPUテストでHistory/Liveの描画対象区別と再配送しないこと、旧main設定の値保持、
設定の上限下限・非有限値・保存再読み込みを確認した。これは実QMLの生成・削除や
アニメーションの検証ではない。実画面、64件上限時と終了後のオブジェクト数、長時間の
RSS・GPUメモリー・フレーム時間、実NX-Jikkyo受信は未検証。
mainの投稿欄風の装飾、言語切り替え、勢い表示、画面全体のデザイン比較も残る。

この段階の検証結果は、実況モデル3件・設定6件のCPUテスト、全ターゲットClippy、
fmt、DanmakuOverlay / ToggleSwitch / ProgramSidebar / SettingsDrawerのqmllint、
CMakeリリースビルドが成功。音声接続問題の解消確認がないため実アプリは起動していない。


## 勢い表示の解析境界（取得・UI接続は未完了）

mainのruntime/fetch.rsとQMLのjikkyoForceを照合した。mainはチャンネル取得時に
NX-Jikkyoの/api/v1/channelsも読み、最初のACTIVEスレッドのjikkyo_forceを採用する。
チャンネルのサイドバーと下部選択一覧の右端に、Activity / 勢いの文言と数値を表示する。
実験版にはこの通信・表示がまだない。

2026-09-07、公開API https://nx-jikkyo.tsukumijima.net/api/v1/channels を読み取り確認。
HTTP 200、JSON応答94,814 bytesで、id: jk1、threads中にPAST / ACTIVE / UPCOMING、
jikkyo_forceは整数またはnullだった。/openapi.jsonはJSONではなく、/api/openapi.jsonは404。
スキーマ取得に成功したとは扱わず、実応答とmainの契約を根拠とした。

viewer-comments/activity.rsにQt・通信に依存しないSnapshot::parseを追加した。
入力上限1MiB、局配列上限256件。根の配列とスレッド列を逐次デシリアライズし、
不要な番組情報・説明をserdeで読み飛ばす。保持するのはBTreeMap<u16, u64>だけで、
局ID・状態文字列は可能なら入力から借用する。エスケープ文字列の場合だけCowが所有する。
最新スナップショットの取得・置換・破棄の管理はまだ実装していない。

最初のACTIVEの値がnullなら、後続のACTIVEを代わりに採用しない（main同様）。
0は有効値、最大u64も欠落させない。jk+1やjk01等はアプリの局対応と一致しないため除外する。
応答過大・局数過多・不正JSONはResultで失敗させ、部分的な成功として返さない。
サーバー本文や全スレッドのVecをスナップショットへ保持しない。

単独crateのネットワーク機能込みCPU・ローカル通信テスト16件が成功。
追加3件は先頭ACTIVEの選択、0/null/u64最大値、サイズ・局数上限、不正JSON、
未対応局ID・不要な情報の除外を確認する。
次の作業は既存Network上の取得タスクの所有・無効化時キャンセル・取得結果の置換と、
main同様の2か所の表示への接続。実況無効時に勢い取得もしないことを検証する必要がある。

単独crateの全ターゲットClippy（network有効、警告をエラー扱い）も成功。


## 勢い取得と一覧への接続

features/comments/activity.rsに既存Networkを使用するActivityを追加した。
Idle / Fetching(Job) / Cancelling(Job)のenumで一つだけの取得タスクを所有する。
無効化でスナップショットを解放・キャンセルし、終了を確認するまで新しい取得を始めない。
無効→有効の高速切り替えでも旧結果を受け入れない。Jobの既存Dropもabortする。
通常終了はキャンセルを要求し、その後の所有者破棄と既存Tokio runtime終了で解放する。

mainのapp.rsが300,000msごとに局・勢いを更新することを確認し、5分間隔を使用した。
新実装は取得完了から5分後に次の要求を開始する。実況有効かつ局一覧取得済みの場合だけ
通信し、動画停止やパネルを閉じることでは受信機能を停止しない。
失敗時はstderrへ理由を記録して勢いを消し、同じ間隔で再試行する。
新しいスナップショットが同一ならQML向けJSONを再生成しない。局一覧変更時は再投影する。

Player.activity_dataは局一覧のindexに対応したnullまたは数値文字列の配列。
既存のjikkyo局対応を共有し、HTTPサービスIDから放送サービスIDを算術推測しない。
u64を数値文字列にしてJavaScriptによる桁落ちを回避する。
mainのActivity翻訳・色・文字サイズを使い、サイドバーと下部選局一覧の見出し右端へ表示する。
未取得は表示せず、0は表示する。実況無効・サーバー変更・終了時には表示も解除する。

ローカルHTTP試験2件で、取得完了・5分未満の再取得抑制・失敗時の消去、完了済み旧結果を
無効→有効切り替え後に受け入れないことを確認。明示的な放送サービスIDによる投影と
u64最大値の文字列化も確認した。既存実況モデル3件と対象QMLのqmllintも成功。
実アプリでのNX-Jikkyo取得・無効化・画面配置、および長時間資源測定は未実施。

全ターゲットClippy（警告をエラー扱い）も成功。試験用HTTPサーバーは2048 bytes上限でヘッダー終端まで読み、部分読み取りを扱う。

CMakeリリースビルドも成功。


## mainの実況パネルの表示補完

mainのQMLと照合し、実況ページ下部の高さ58px・角丸20pxの入力欄風表示と送信アイコン、
再生バーの選局と字幕の間にある鉛筆アイコンを移植した。SVGはmainから同一ファイルを
コピーし、Qtリソースへ登録した。見出し・タブのComments、Danmaku、Collapseはmainの
翻訳コンテキストへ揃える。

mainのこれらの投稿用表示には入力・送信の実装がない。移植先でも入力状態や通信処理を
追加せず、表示だけを再現する。投稿機能の完成を意味しない。
既存の受信・200件上限の履歴・非表示時のListView解放は変更しない。

[Qt Quick Layoutsの公式仕様](https://doc.qt.io/qt-6/qtquicklayouts-overview.html#specifying-preferred-size)
に従い、ColumnLayout配下の高さはLayout.preferredHeightで指定する。
幅・高さをレイアウトと二重に制御しない。ProgramSidebarのqmllintとdiff・fmt検査が成功。
実画面の比較、縮小時や英語表示時のレイアウトは未検証。

CMakeリリースビルド（QMLの事前コンパイルとアイコンのリソース組み込みを含む）も成功。


## 再生設定ポップアップ

mainのPlayback settingsは全般設定Drawerとは別の320×340pxポップアップ。
実験版の再生バーが全般設定を開いていた差を修正し、同じ映像領域右下の配置、背景色、
角丸、余白でPlaybackSettingsを分離した。実況表示、14〜36pxの文字サイズ、20〜100%の
透明度、0.5〜2倍の速度、動画統計の切り替えを既存Rustプロパティと操作へ接続する。
統計切り替えではmain同様にポップアップを閉じる。閉じた時に既存の変更分保存を呼ぶ。

DanmakuAdjustmentsは全般設定と共有する表示・操作部品で、required real値と変更シグナルを
持つ。独自の設定コピーやTimerを追加しない。Rust側の有限・範囲検証を通した値が表示の
正となる。実況無効時は調整操作を無効にし、機能を勝手に起動しない。

[Qt Popupの仕様](https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#closePolicy-prop)
に合わせ、Overlay.overlayを親とし、focusを設定してEscapeと外側クリックで閉じる。
既存のポップアップ監視により、開いている間のC/G/選局ショートカットを抑制する。

3部品のqmllint、fmt・diff検査が成功。実入力での開閉・保存、日英の実画面比較は未検証。

QML事前コンパイルを含むCMakeリリースビルドも成功。


## 地上波以外の対応付けの照合

mainのviewer-core/comments.rsはGR以外を放送serviceIdで対応付ける。
実験版だけBand::Otherを一律除外していた差を修正し、明示的な放送情報があるOtherも
同じ規則（102は101へ対応）を使用する。情報がない場合はNoneとし、HTTP配信用IDや
局名からserviceIdを推定しない。勢い表示も同じ対応関数を共有する。

Otherでの101/102対応と放送情報欠落の試験を追加した。アプリ側5試験、単独crateの通信・
終了・プロトコル16試験が成功。終了待ち・最終コメントの上限付き排出・無効化時の旧接続
解放は既存実装で確認できた。実サービスで対象の実況が提供されることを保証する変更ではない。

全ターゲットClippy、fmt・diff検査、CMakeリリースビルドも成功。実再生との併用検証は未実施。


## 履歴JSONの中間配列を除去

Comments::jsonは履歴を借用するSerialize実装から行を逐次書き出す。
[Serde Serializer::collect_seq](https://docs.rs/serde/latest/serde/ser/trait.Serializer.html#method.collect_seq)
でiteratorを配列としてシリアライズし、200件のRowと時刻文字列を一括保持するVecを除いた。
時刻文字列は各行の処理中だけ保持する。本文は従来どおり借用し、最終JSON文字列と
Qtへの変換、QMLの表示データは引き続き割り当てる。履歴・受信・ライブ通知の上限は変更しない。

既存の履歴上限／切り替え／無効化試験を補強し、最後の200件の到着順、日本時刻、
改行・日本語・タグを含む本文、NXの出所表記、無効化時の空配列を確認する。
非表示時にJSONを生成しない既存条件を維持する。実実況併用時のRSS・処理時間は未測定。

実況機能の3試験とClippy全ターゲット、fmt・diff検査が成功。
CMakeリリースビルドも成功。実サービス受信と長時間併用の再検証は残る。


## 実実況サービスでの受信と停止

2026-09-07、mainと同じNX実況のjk101（NHK BS）のthreads APIとコメントWebSocketへ、
本体のConnectionを用いて30秒接続した。Receivingへの遷移、履歴100件、新着0件、
キューあふれ0件を観測し、stop().wait()は5秒の期限内に完了した。
本文を保存・出力せず、50msごとに最大64件を消費して件数のみ保持した。

任意試験real_service_reception_and_stopは、接続先をCOMMENT_THREADS_URLと
COMMENT_STREAM_URLで明示して--ignoredで実行する。通常試験では外部サービスへ接続しない。
HTTP／WebSocketの接続・購読要求は本体の経路を使い、コメント投稿は行わない。
異常の観測時もタスク終了を待ってからエラーを返す。表示・GPU・音声は使用しない。

```sh
COMMENT_THREADS_URL=https://nx-jikkyo.tsukumijima.net/api/v1/channels/jk101/threads \
COMMENT_STREAM_URL=wss://nx-jikkyo.tsukumijima.net/api/v1/channels/jk101/ws/comment \
CARGO_TARGET_DIR=build/comments-protocol cargo test --locked \
  --manifest-path rust/crates/viewer-comments/Cargo.toml --features network \
  real_service_reception_and_stop -- --ignored --nocapture
```

任意試験とnetwork有効・全ターゲットClippy、fmt・diff検査が成功。
新着コメントの到来、Qt一覧／動画上の描画、選局での切り替え、再接続、長時間メモリーは
今回の検証に含まない。機能本体の処理変更はなく、アプリ再ビルドは行っていない。


## 停止タスクの結果回収（2026-09-07）

ControllerがStopping.is_finishedだけで次の状態へ進む実装を修正した。
従来も旧タスクの終了前に次を開始することはなかったが、終了したJoinHandleを捨てるため
panicのJoinErrorを呼び出し元で受け取れなかった。
[Tokio JoinHandle](https://docs.rs/tokio/latest/tokio/task/struct.JoinHandle.html)の
終了確認と結果取得の違いに基づき、EPGイベント購読と同様のtry_finishを追加した。

Pendingではハンドルを保持し、完了時に結果を一度だけ回収してハンドルを除く。
明示的なキャンセルは成功、panicはsource付きcontroller::Error::Taskで返す。
Network、CommentsをResultで経由してPlayerが診断出力する。購読が引き続き必要なら
既存の5秒待ちを使い、異常終了を受け取ったpoll内で即時再起動しない。
既存のGUI pollを使い、待機用スレッド・タイマー・キューは追加しない。
これは異常終了の情報欠落の修正であり、メモリーリークの発見や改善の証明ではない。

非同期キャンセル処理前のPending、キャンセル完了、panicの判別、回収後に再pollしても
同じハンドルを再pollしないことを試験した。実況crateは17件成功・外部サービスの
手動試験1件除外。既存の急速な接続先変更、無効化、HTTP待ち・idle WebSocketの解放、
再接続期限、最終コメントの64件ずつの排出も含む。crateの全ターゲットClippyも成功。

アプリ全ターゲットClippyとreleaseビルドも成功。専用Xvfb :99の実アプリで
実況受信中→機能無効→再有効後の受信中への復帰を画像で確認した。
証跡はbenchmark/virtual-ui/comment-join.logとcomment-join-*.png。
実サービスでのpanic再現、新着コメント描画、長時間併用の検証を代替するものではない。

## Qt描画オブジェクトの寿命試験（2026-09-07）

DanmakuOverlayを直接生成するQMLテストを追加した。専用Xvfb :99 / llvmpipeで、
1,000件のreceive要求でもliveEntriesと実際の子Itemが64件までであることを確認。
clearComments直後は所有配列・レーン参照が空になり、Qtの遅延destroy後には子Itemも0件になる。
同じスクリプト内でクリア直後に1件追加しても、その新しい子Itemが旧世代の破棄に巻き込まれない。
二重クリア、非表示時の解放、非表示中の受信を無視すること、再表示後の受信も確認した。
HTML風の入力はPlainTextのまま。実際のNumberAnimation完了後にも所有配列・レーン参照・
子Itemが空になることを確認した。速度は設定範囲内の2、フォントサイズ21。

新規部品試験は5件（初期化・終了を含む）、全QML試験は84件成功。製品コードの変更なし。
試験ファイルはrust/qml/tests/tst_DanmakuOverlay.qml、結果はGit対象外の
benchmark/virtual-ui/danmaku-lifetime-tests.txt・danmaku-all-tests.txt。
オブジェクト所有と寿命の試験であり、Qt内部キャッシュ・プロセスRSS・GPU資源の
長時間安定性、実サービスの再接続を証明するものではない。


## 実サービスとGUIの有効・無効切り替え（2026-09-07）

製品コード568a853を検出・検証済みXvfb :99 / llvmpipeで起動し、実Mirakurunの
NHK総合1京都を再生。字幕無効、EPG・実況・流れる表示を有効、明示したfakesinkで
音声は破棄した。専用設定・stateを使用し、通常画面には入力を送っていない。

実実況サービスから履歴を受け取り、サイドパネルの一覧と映像上の流れる表示を確認。
診断では開始約33秒に履歴132件・表示中11件。設定で実況を無効化した後、
約40.5〜70秒のsampleは履歴0・表示中0で、画面にも「無効」を表示した。
無効化のプロパティ変更通知直後の記録は解放前の履歴143件を含むため、
完了判定にはその後のsampleと画面を使用した。

続いて約150ms間隔の連続ON/OFFで最後を有効にし、約81秒に履歴104件・表示中4件を
確認した。実画面でも受信一覧と流れる表示が戻った。勢い取得のHTTP成功もログで確認。
診断の破棄数0、QML例外・GStreamer critical・接続失敗の出力なし、閉じるボタンで終了コード0。

証跡はGit対象外のbenchmark/comments-live-ui/（player.log、各png、summary.json、
state/mirakurun-viewer/usage/usage-95444.jsonl）。製品コードの変更はない。
機能OFF→ONによる再接続の確認であり、ネットワーク切断後の自動再接続や
長時間・全機能併用・音声聴取・GPU性能を検証したものではない。
