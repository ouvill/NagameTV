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
