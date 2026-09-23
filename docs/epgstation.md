# EPGStationの録画一覧・再生

「録画を開く」→「EPGStationの録画」から、サーバー上の録画を検索して再生できます。
Mirakurunの接続設定は不要です。

## 接続する

1. EPGStationのURL（例: `http://epgstation:8888`）を入力します。
   サブディレクトリーで公開している場合は、そのパスも含めます。末尾の`/api`は付けません。
2. 認証なしの場合は「接続する」を選びます。stuayu版で認証を使う場合は「ログイン…」を選び、
   ユーザー名とパスワードを入力します。
3. 一覧の取得に成功すると接続先URLを保存します。検索欄でキーワードを指定し、
   「前へ」「次へ」で50件ずつ移動できます。キーワードの検索対象・並び順はサーバーの仕様に従います。
4. 番組の「再生」を選びます。TSの検証に成功すると録画一覧を閉じ、視聴画面へ移ります。

パスワード、セッションCookie、再生トークンは設定ファイルへ保存しません。
認証状態はアプリ起動中に保持し、再起動後は再度ログインします。別の接続先へ切り替えた場合も
認証情報を引き継ぎません。通信エラーや認証エラーで一覧を取得できなかった接続先は保存しません。
設定の保存に失敗した場合は画面にエラーを表示します。

## 対応範囲

| 接続先・機能 | 対応 |
| --- | --- |
| 本家EPGStation v2の共通API | 認証なしの録画一覧・検索・録画済みTS再生 |
| stuayuフォーク版 | 共通APIに加え、ユーザー名・パスワード認証と外部プレイヤー用トークン |
| 再生 | 録画が完了した、サイズが0より大きいTSファイル。サーバーのHTTP Range対応が必要 |
| 録画中・TSファイルなし | 一覧に理由を表示し、再生ボタンを無効化 |

MP4などのエンコード済みファイル、録画中の追いかけ再生、SSO、リバースプロキシ独自の認証、
録画予約・削除・視聴履歴の同期は未対応です。複数のTSファイルがある場合は、APIが返した順に
最初のサイズが0より大きいTSを再生します。字幕・実況・シークには既存の
[HTTP録画再生](recording-playback.md#epgstationの録画url)を使います。

## APIと所有関係

2026-09-23に参照したstuayu版のコミットは
[`26f56720722e61e66fc33fc0c4aacfd3d98913ba`](https://github.com/stuayu/EPGStation/tree/26f56720722e61e66fc33fc0c4aacfd3d98913ba)です。
ユーザー環境の「最新版」は更新されるため、ここでは調査対象をこのコミットに固定します。
本家版は[`5cf2ea383d37937eacecf424820dbd7a278d577e`の録画API](https://github.com/l3tnun/EPGStation/blob/5cf2ea383d37937eacecf424820dbd7a278d577e/src/model/service/api/recorded.ts)と
[型定義](https://github.com/l3tnun/EPGStation/blob/5cf2ea383d37937eacecf424820dbd7a278d577e/api.d.ts)の共通部分を使います。

- `GET /api/recorded?isHalfWidth=false&offset=…&limit=50&keyword=…`で一覧を取得します。
- stuayu版のログインは`POST /api/auth/login`、再生トークンの取得は`GET /api/auth/media-token`です。
  Cookieは接続先ごとのHTTPクライアントで管理し、API要求のリダイレクトには追従しません。
- 再生は`GET /api/videos/{videoFileId}`を使用し、認証時は`token`クエリーを付けます。
  録画番組IDから現在の一覧の動画ファイルIDを解決し、URLをQMLへ公開せず録画ローダーへ渡します。

Qtに依存しない`epgstation::Library`が一覧、ページ、認証セッション、通信の状態を所有します。
通信中・取消し待ち・待機中をenumで分け、取消したJobの終了を確認してから次の要求を開始します。
一覧を正常に解析したときだけ生成できる`VerifiedEndpoint`を設定保存に使います。
`RecordingModel`は読み取り専用のQt投影で、番組IDは文字列として公開します。

JSONの取得上限は4 MiB、1回のHTTP要求は10秒、接続は5秒で打ち切ります。
検索文字列は256文字までです。一覧の操作は再生中のセッションを変更せず、
「再生」を押してTSの検証に成功したときに再生対象を切り替えます。

## 検証

ローカルのHTTP応答を使うRustテストで、共通API、認証Cookie、再生トークン、
検索・ページ移動、接続先間の認証分離、取消し、遅延応答、HTTPエラー、不正なJSONを確認します。
接続テストではQtの一覧モデル、通知中の整合性、確認済み接続先の保存を扱います。
起動テストでは製品の画面から録画一覧を開き、既存のHTTP録画再生へつなぎます。

```sh
CARGO_TARGET_DIR=build/cargo cargo test --manifest-path rust/Cargo.toml --release --locked epgstation
bash scripts/test-connection.sh
bash scripts/test-danmaku.sh
bash scripts/test-startup.sh
```

2026-09-24に、Rustテスト320件（除外5件）、QML部品テスト312件（対象外18件）、
接続・デスクトップメディア・起動テストの成功を確認しました。起動テストでは認証なしと
Cookie／再生トークンを使う接続の両方を再生し、640×360と960×540の画面を確認しています。
確認画像は`build/navigation-review/epgstation-*.png`へ出力します。

利用中の実サーバーへの接続確認は未実施です。APIの互換性を調べた範囲と、
実サーバーの設定を含めた動作確認は区別します。
