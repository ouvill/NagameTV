# EPGStationから取得したAPI応答

このディレクトリーのJSONは、[固定版の情報](../../epgstation/provider.json)にある
stuayu/EPGStationのHTTPサーバーをDocker内で動かして取得した応答です。
2026-09-26に、コミット`26f56720722e61e66fc33fc0c4aacfd3d98913ba`から取得しました。
番組名・時刻・動画メタデータはテスト用の値で、利用者の録画データではありません。

[初期化スクリプト](../../epgstation/bootstrap.cjs)が本体のDBマイグレーションと
ORMを使って52件の録画とチャンネルを登録し、本体の`IServiceServer`を起動します。
HTTPルート、認証、DB検索、JSON生成、ファイル配信は本体の実装です。
録画・EPG更新・エンコード・機器検出のプロセスは起動しません。
動画メタデータは解析済みの状態として登録するため、ffprobeによる解析精度も対象外です。

| ファイル | 取得した要求 |
| --- | --- |
| recorded.json | `GET /api/recorded?isHalfWidth=false&offset=0&limit=50&keyword=EPGStation%20recording` |
| channels.json | `GET /api/channels` |
| video-124-metadata.json | `GET /api/videos/124/metadata` |

録画一覧には検索に一致した12件が入っています。動画ファイルは隣の
`recording-seek.ts`、`media-h264.mp4`、`media-hevc.mkv`です。
JSONの整形以外に応答の変更はしていません。認証応答・Cookie・トークンは保存しません。

通常の検査では、毎回新しいDBを作り、認証なしと認証付きの実応答を保存済みJSONと比較します。
差があれば失敗し、自動では更新しません。

```sh
CARGO_TARGET_DIR=build/cargo python3 scripts/epgstation-integration.py
```

意図して提供元や入力データを変更した場合のみ、次のコマンドで再取得し、差分をレビューします。
実サーバーのテストも同時に実行します。失敗した場合は更新済みJSONを検証済みとして扱わないでください。

```sh
CARGO_TARGET_DIR=build/cargo python3 scripts/epgstation-integration.py --update-fixtures
```

起動用モックではこのJSONを再利用します。検索や認証の簡略化、障害の注入はモック固有の処理です。
実サーバーとの比較試験で一覧・チャンネル・メタデータのJSONと、動画のステータス、
成功時のContent-Type・Content-Range・Content-Length・本文を検証します。
未知のルートは成功扱いにせず404を返します。
