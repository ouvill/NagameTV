# 実況機能の移植

移植元はmainのrust/src/comments.rs、viewer-core/src/comments.rsとapp.rs、
Main.qmlのdanmakuLayer・実況一覧・設定。目標は受信だけでなく、接続の世代管理、
チャンネル対応、再接続、履歴一覧、画面上の実況、各設定とmainの表示デザインを再現すること。
この文書の解析crate追加時点では、実験アプリからの接続・表示はまだ有効になっていない。

## プロトコルの分離

rust/crates/viewer-commentsはQt・GStreamer・デバイス・非同期実行環境に依存しない。
通信の解析・購読リクエストの生成を先に切り出した。今後の受信タスクはこのcrateを利用し、
その結果をアプリの接続状態とQtの表示モデルへ渡す。
単独crateにすることで、再生環境を使わず通信データの契約を検証できる。

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
