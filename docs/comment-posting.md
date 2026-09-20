# NX-Jikkyoへのコメント投稿

再生バーの鉛筆ボタンで、映像領域の下部中央に一行の投稿欄を開く。
Penpotの「05 Live · Composer」に合わせ、高さ46px・最大幅760px・下端余白16pxとし、
入力欄・文字数・送信アイコンを表示する。投稿欄を開いている間は再生バーを隠す。
Escapeまたは映像部分のクリックで閉じる。サイドパネルを開かずに投稿できる。
標準はCtrl+Enterで送信。設定の「Enterでコメントを送信する」を有効にすると
Enterでも送信できる。設定は再起動後も復元する。一行入力のため改行キーは改行を挿入しない。
テンキーのEnterにも対応する。日本語入力の変換中とキーの自動リピートでは送信しない。
送信ボタンも利用できる。
変換中の判定には入力欄の`preeditText`を使う。Qtの`inputMethodComposing`は
未確定文字がなくてもカーソル・書式属性だけで真になり得るため、確定済みの本文の
送信をそのフラグで止めない。投稿欄内の余白や無効な送信ボタンへのクリックは
欄内で受け止め、背後の映像のクリックによって投稿欄を閉じない。
参照: [Qt TextInputのIME状態処理](https://github.com/qt/qtdeclarative/blob/v6.10.2/src/quick/items/qquicktextinput.cpp)。

投稿先は視聴中の放送局に対応するNX-Jikkyoのjkチャンネル。投稿欄にチャンネル名は表示しない。
匿名・白・横流れ・標準サイズで投稿する。
本家ニコニコ実況への投稿やアカウント連携は含まない。
局の対応表は既存の受信機能と共有し、実況側の番組名はパネル上部に表示する。

## 下書きと送信状態

- 入力欄は最大1024 UTF-16コード単位（サロゲートペアの途中では切らない）、通信層は4096 UTF-8バイトまで。
- 空白だけの投稿を拒否する。本文はプレーンテキストで、貼り付けた改行は空白に変換し、絵文字・引用符は保持する。
- 下書きはアプリのメモリーに保持し、投稿欄を閉じても残す。設定ファイルやログには保存しない。
- 送信中は入力を読み取り専用にし、重複送信を拒否する。
- 対応する投稿結果を受け取ったときだけ下書きを消す。画面のコメントは既存の受信経路で表示し、投稿時に重複挿入しない。
- 送信結果は投稿欄の上に表示する。成功通知は3秒で消し、失敗・結果不明の案内は次の編集で隠す。入力欄と結果表示の領域には弾幕を重ねない。
- 失敗・結果不明では下書きを残す。送信後に接続が切れた場合などは結果不明とし、再送前に履歴を確認する案内を出す。
- 放送サービスの変更・実況無効化・接続先サーバーの変更では下書きと投稿状態を解除し、旧投稿タスクを取り消す。送信済みの投稿をサーバーから取り消せるという意味ではない。

## 所有関係と通信

`viewer-comments::posting`はQt・GStreamerに依存しない。
`protocol`が応答の型・本文検証・時刻の解析・リクエスト生成、`transport`がWebSocket通信、
`Controller`が投稿の所有権・取消し・間隔制限を担当する。
アプリ側の`features/comments`が投稿先を解決し、`player/comment_posting`がQtとの境界を持つ。
`CommentComposer.qml`は入力と表示を担当する。

送信操作1回につき、既存のTokio runtimeで1つの投稿タスクを開始する。
`/api/v1/channels/jk{id}/ws/watch`に接続して`startWatching`を送信し、
`serverTime`と`room.vposBaseTime`から10ms単位のvposを計算する。
サーバー時刻受信後の経過時間は単調時計で補う。投稿先と本文は開始時に固定する。
`postComment`は1回だけ送信し、本文が一致する`postCommentResult`を確認して接続を閉じる。
接続を毎回確立する分の通信待ち時間があるが、入力していない間に投稿用接続を維持しない。

試行全体は10秒で打ち切り、フレーム・メッセージは64KiBまで。
NXの座席維持間隔30秒より短い試行で、JSONのpingにはpong/keepSeat、
WebSocketのPingにはPongを返す。試行完了から最低1秒空けて次の投稿を許可する。
接続・投稿の自動再試行や未送信キューは持たない。
取消し完了を観測するまで新規投稿を受け付けず、旧世代の成功通知は新しい下書きに反映しない。
Dropでもタスクをabortする。

NXは連投制限時に成功応答を返しながらコメントを破棄する場合があるため、
成功表示は投稿APIの応答確認を意味し、全視聴者への配信を保証しない。

参照: [NX-Jikkyo WebSocket API](https://github.com/tsukumijima/NX-Jikkyo#websocket-api)、
[サーバー実装](https://github.com/tsukumijima/NX-Jikkyo/blob/master/server/app/routers/websocket.py)、
[公式クライアント](https://github.com/tsukumijima/NX-Jikkyo/blob/master/client/src/services/player/managers/LiveCommentManager.ts)。

## 検証

通信テストはランダムポートの127.0.0.1へ接続する。本番の投稿先は使わない。
成功・拒否・不正応答・本文不一致・サイズ上限・時刻同期・タイムアウト・切断・
重複送信防止・選局/無効化/Dropによる取消しを検証する。
既存の公開サービス受信テストは引き続きignoredで、投稿テストを公開サービスに切り替える設定はない。

```sh
CARGO_TARGET_DIR=build/comments-protocol cargo test --locked --manifest-path rust/crates/viewer-comments/Cargo.toml --features network
CARGO_TARGET_DIR=build/comments-protocol cargo clippy --locked --manifest-path rust/crates/viewer-comments/Cargo.toml --all-targets --features network -- -D warnings
CARGO_TARGET_DIR=build/cargo cargo test --manifest-path rust/Cargo.toml --release --locked
```

QMLテストの`CommentComposer`はCtrl+Enter/Enter/Shift+Enter、送信ボタン、無効状態、
入力上限、下書き更新、日本語のpreedit/commitイベントを確認する。
テスト用イベント配送部品は`qml_tests`でのみビルドし、通常版には含めない。
画面テストには検出・検証済みの表示環境を使う。入力操作は専用の表示先で行う。

2026-09-13の検証結果:

- `viewer-comments`のローカル通信・コア試験36件成功、公開サービス受信試験1件は実行せず。
- アプリのRust試験144件成功、手動試験3件は実行せず。
- QML試験148件成功、警告なし。今回の入力欄5試験と送信設定の切り替え試験を含む。
- X11 :0とNVIDIA GeForce RTX 4070 Tiの動作を検出・検証。
  入力操作は専用Xvfb :98 / llvmpipeで実施した。UIの振る舞いを確認する試験であり、GPU性能の測定ではない。
- 実際のNX-Jikkyoへの投稿は実行していない。

全ターゲットClippy（`qml_tests`を含む）と実況crateのClippyは警告なし。
検査で見つかった既存の字幕試験用fixtureの重複読み込みを共通化し、
PSI解析ループを同じ終了条件のwhile-letへ整理した。変更後も字幕を含むRust試験144件とQML試験148件が成功。
投稿UIのqmllintとRustの書式・差分検査も成功。

検証ログと入力欄の画像はGit対象外の`benchmark/comment-posting-20260913/`に保存した。
CMakeリリースビルドも成功し、`build/mirakurun-viewer`を更新した。

## 接続通知の解析修正（2026-09-13）

実投稿時に`invalid type: map, expected unit variant Event::Ignore`で失敗したとの報告を受け、
`seat`通知のJSONで同じエラーをローカル再現した。
`type`と`data`を隣接タグとして扱うenumでは、未知種別のunit variantがオブジェクトの
`data`を受理できなかった。初期テストには座席・スケジュール・統計通知が欠けていた。

メッセージ全体の`type`で分岐する形式へ変更し、使用する通知の`data`だけを型付きで解析する。
`seat`・`schedule`・`statistics`・未知種別の通知は本文形式によらず読み飛ばす。
既知の通知の不正な必須項目と不正JSONは引き続きエラーとする。
`ping`も`data`なしと空オブジェクト付きの両方を受理する。

本番と同じ`serverTime → seat → schedule → room → statistics`の順序で応答する
ローカルWebSocket試験を追加し、投稿と応答確認まで検証した。
既存の逆順通知・拒否・切断・結果不明・取消し試験にも接続通知を加えた。
実サービスへの投稿テストは行っていない。

修正後の実況crate試験39件とClippyが成功。CMakeビルドも成功し、起動用バイナリを更新した。
検証ログは`benchmark/comment-posting-20260913/envelope-*.log`に保存した。

## 下部の一行入力（2026-09-13）

Penpotの「FluTV Redesign / 05 Live · Composer」を参照し、投稿欄をサイドパネルから
映像領域の下部中央へ移した。チャンネル表示を省き、文字数と送信アイコンを一行にまとめた。
デザイン中の120という上限表示は、実装の上限1024に合わせている。
入力中は再生バーを隠し、投稿後も入力を続けられる。Escapeまたは映像クリックで閉じ、下書きは保持する。

QML試験151件成功、警告なし。一行化に伴う改行の空白化、下書き同期、文字数、
幅340/492/760pxでの入力・文字数・送信ボタンの非重複も確認した。
CommentComposer / ProgramSidebar / SettingsDrawerのqmllintも警告なし。
専用Xvfb :98で通常幅・狭い幅の入力欄を描画し、表示を確認した。
検証ログと画像は`benchmark/comment-posting-20260913/compact-*`に保存した。
NX-Jikkyoへの投稿は実行していない。

CMakeリリースビルドが成功し、`build/mirakurun-viewer`を更新した。
接続先を空にした専用の起動画面でも、鉛筆ボタンでの表示・直接入力・Escapeでの終了・
再表示時の下書き保持・映像部分クリックでの終了を確認した。起動・操作中のQML警告はない。

## 成功通知の期限と自分の弾幕（2026-09-13）

成功応答をGUIが確認してから3秒後に成功通知を解除する。既存のpollで単調時計を確認するため、
文言やUI言語への依存はなく、投稿欄を開き直しても古い通知を再表示しない。
失敗・結果不明の案内は自動で消さない。

自分の投稿も通常の受信経路から弾幕へ流れ、その項目だけ透明背景・1pxの黄色枠（#ffe066）で囲む。
投稿時の合成コメントは追加しない。弾幕OFFや再生停止中には描画しない。

視聴セッションのroomにあるthreadIdとyourPostKey、実際に送信したvposと本文を一時保持し、
受信したNXコメントのthread・user_id・vpos・本文がすべて一致したときに一度だけ自分の投稿と判定する。
送信前に照合情報をGUIへ公開するので、コメントの受信が成功応答より先でも識別できる。
本文のみの照合は行わず、識別情報がない応答でも投稿は継続して通常表示とする。
照合情報は最大64件・60秒で、選局・無効化・接続先変更時に破棄する。
受信側の識別情報はコメント履歴のJSONには含めない。

枠の分を文字幅の計測に含め、既存の弾幕配置・移動・寿命・透明度に従わせる。
検証にはループバック通信と専用の表示先を使用し、公開NX-Jikkyoには投稿しない。

検証: 実況crate42件、アプリRust144件、QML152件と描画採取3件が成功。
成功通知の期限、成功応答前の受信、同本文・別投稿者・別vpos・別スレッドの除外、
過去コメントの除外、照合期限、選局時の破棄を確認した。
投稿識別の不足・不正な時刻情報は装飾を省くだけとし、通常コメントの表示を妨げない。
実況crateとアプリ全ターゲットのClippy、および変更QMLのqmllintは警告なし。
ログと黄色枠の画像は`benchmark/comment-posting-20260913/own-*`に保存した。

CMakeリリースビルドも成功し、`build/mirakurun-viewer`を更新した。公開サービスへの投稿は実行していない。
