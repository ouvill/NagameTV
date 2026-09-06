# Playerの責務と所有権

`rust/src/player.rs` はQMLに公開するCXX-Qtブリッジを定義する。
QMLのプロパティ・シグナル・呼び出し名は維持し、処理は `rust/src/player/` に分割した。

| モジュール | 責務 |
| --- | --- |
| `state` | QObjectが所有するRustの状態、初期化 |
| `playback_control` | 再生・停止・音声選択・イベント処理、再生エラー型 |
| `subtitles` | 字幕更新とQML用データへの変換 |
| `comments` | 実況接続・受信処理、上限付きの履歴型 |
| `fetch` | HTTP取得・応答の検証・公開前のEPG構築、取得エラー型 |
| `catalog_request` | 取得タスクと結果キューの所有、取消し・完了処理 |
| `catalog` | EPG更新・イベント購読・現在番組の更新 |
| `catalog_view` | 受け取ったカタログをQMLプロパティへ反映 |
| `selection` | 選局とチャンネル表示の更新 |
| `preferences` | サーバー変更・言語変更・設定保存 |
| `telemetry` | 使用状況の記録、再生エラーの表示・保存 |

## EPG取得の状態遷移

動的に切り替わる取得状態を `CatalogRequest::Idle / Loading` で表現する。
`Loading` はサーバー、取消し時にabortする `NetworkTask`、容量1件の結果キューを所有する。
取得中かどうかを表す別のAtomicBoolは持たない。

- 完了結果をGUIの50msタイマーで取り出すとIdleへ戻る。
- サーバー変更ではタスクと結果キューを破棄し、公開済みEPGも空にしてから新しい取得を始める。
- A→B→Aの変更でも、古いAの結果は新しいリクエストのキューへ入れない。
- キューの送信者が結果を返さず消えた場合は `FetchServicesError::WorkerStopped` として扱う。
- Player破棄時にもタスクの所有者が破棄される。単にJoinHandleを捨ててタスクを切り離さない。

HTTPワーカーはリクエスト専用のEPGを構築する。共有ストアを直接変更しない。
GUI側が有効な結果を受け取ったときだけ、完成済みの `Arc<EpgSnapshot>` を公開する。
既存の読み手は従来の不変スナップショットを読み続けられる。

## Qtとの境界・メモリー

QObjectのプロパティ変更はGUIスレッドに限定する。ワーカーがキューへ渡すのはRustデータのみ。
QMLのタイマーが停止しても、EPG結果は最大1件で、無制限には積み上がらない。

`CommentHistory` が履歴200件の上限を管理する。実況は従来どおり受信キュー256件、
1回の処理64件、表示64件に制限する。履歴を構成する文字列は名前付き `CommentEntry` で表す。
診断用のUI状態・再生オプションも、位置で意味が決まるタプルから名前付きの型へ変更した。

選局時はQStringを直接cloneする。Qtの暗黙的共有を利用し、不要なUTF-8への変換と再確保を減らす。
番組表の文字列も参照からQStringへ変換し、変換直前のRust文字列cloneを省く。
これらは割当削減と所有権の改善であり、視聴中のRSS増加原因を確定するものではない。
長時間の実測には[診断ログ](passive-diagnostics.md)を利用する。

分割したPlayerコードにpanicする `unwrap()` / `expect()` はない。
HTTP、時計、JSON変換はResultで処理し、Qt境界で表示・診断ログへ変換する。
`unwrap_or` / `unwrap_or_default` は欠損値の明示的な既定値でありpanicしない。
EPGストアのロックがpoisonされた場合も、完成済みArcしか代入しない不変条件に基づいて回復する。

CXX-Qtが同じヘッダーを複数の生成ディレクトリへコピーするビルドでも、再定義しないように
ローカルC++ヘッダーには名前付きinclude guardを使用する。

## 検証と参照仕様

CPU・メモリー上の単体テストで、取消し後のタスク終了、キュー内スナップショットの解放、
旧リクエスト結果の拒否、公開前後のスナップショット分離、履歴の上限と順序を検証する。
デフォルトのRustテストには音声デバイスを使わないPCM処理と、映像出力を使わないTS分離も含む。
実画面・実音声の視聴試験や、長時間のリーク判定を代替するものではない。

- [Qt: Threads and QObjects](https://doc.qt.io/qt-6/threads-qobject.html): QObjectのスレッド所属とイベント処理。
- [Qt: Implicit Sharing](https://doc.qt.io/qt-6/implicit-sharing.html): QStringなどの参照共有とcopy-on-write。
- [Tokio: JoinHandle](https://docs.rs/tokio/latest/tokio/task/struct.JoinHandle.html): Dropだけではタスクを取り消さず、abortは取消しを要求する。
- [Qt: JavaScript memory management](https://doc.qt.io/qt-6.10/qtqml-javascript-memory.html): QMLのJSヒープとネイティブ割当の区別。

## チャンネル切り替え

選局の判断は `selection/policy.rs` の `SelectionAction` に分離する。
同一サービスが接続中・再生中なら `Keep` として戻り、再接続、設定保存、実況再起動、
字幕や音声のリセットを行わない。停止・エラー時は `Start` として同じ局にも再試行できる。
未選局または現在局が一覧にない場合、「次」は先頭、「前」は末尾を選ぶ。

再生開始は `playback/session.rs` の処理を通り、次の順序を守る。

1. Busをflushし、旧ストリームをREADYまで停止してからflushを解除する。
2. 旧音声ストリームの参照、字幕待ち行列、TSのPES/PSIバッファー、ARIBデコーダーを破棄する。
3. 選択した局の現在番組から取得した `AudioProgram` を設定する。
4. URIを変更し、PLAYINGへの遷移を開始する。

EPG検索は選択したサービスのスケジュールを二分探索し、その音声情報だけを複製する。
番組未取得・番組の空白時間は `None` とし、以前の局の情報を持ち越さない。
1秒周期の番組更新は引き続き番組境界を反映するが、初回の音声情報設定はそれを待たない。
リセットに失敗した場合はResultで再生開始を中断し、既存のエラー表示・停止処理へ渡す。

GStreamerの[状態遷移仕様](https://gstreamer.freedesktop.org/documentation/additional/design/states.html)では
PAUSED→READYでストリーミングスレッドを停止し、動的padを削除する。
[READY/NULLへの遷移はASYNCを返さない](https://gstreamer.freedesktop.org/documentation/gstreamer/gstelement.html#gst_element_set_state)。
そのため旧ストリーム停止後に新しい状態を設定する。Qtの映像sinkとGLコンテキストは再利用し、
通常停止・破棄時はNULLへ戻す。QObject/QQuickItemの操作は
[Qtのスレッド規則](https://doc.qt.io/qt-6/threads-qobject.html)に従いGUIスレッドに維持する。

回帰テストは選局・再試行・折り返し・EPG時間境界、再生開始前の音声情報設定、
旧音声ストリーム参照の解放、字幕状態の初期化を確認する。
ローカルHTTP配信を使った5回の反復テストでは、実際のsouphttpsrcとqueueを通して
READYによる応答受信の中断、ソケット切断、キューの空化、Bus内の旧参照解放を確認する。
このテストは明示的なテスト用fakesinkでバイト列を消費し、映像・音声デバイスを使用しない。
実Mirakurunのチューナー解放完了時間、実画面・実音声、長時間のRSS推移の測定を代替しない。
