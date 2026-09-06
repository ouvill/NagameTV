# 型付きエラーとEPG取得状態の整理（2026-09-07）

mainの後継に向けた最初の整理として、通信・解析・再生のエラー型と、EPG取得状態の
所有関係を変更した。設計は [architecture.md](architecture.md) を参照。

## 自動検証

- `cargo fmt --manifest-path rust/Cargo.toml -- --check`
- `CARGO_TARGET_DIR=build/cargo cargo clippy --manifest-path rust/Cargo.toml --release --locked --all-targets -- -D warnings`：警告なし。
- `CARGO_TARGET_DIR=build/cargo cargo test --manifest-path rust/Cargo.toml --release --locked`：29件成功、外部TSを要求する任意テスト1件は未実行。
- `cmake --build build`：成功。

通信の回帰テストではローカルHTTPサーバーを使い、HTTP 503、JSON構文不正、応答上限超過、
有効JSONだがTVチャンネルなし、を別のエラーとして検証した。HTTP/JSONのsourceも保持する。
EPGのテストでは更新置換に加え、解析失敗時の旧スナップショット保持、失敗後の5分更新期限、
取消し完了後の無効化を確認する。期限判定には時刻を注入し、実際に5分待つ必要をなくした。
既存のHTTP途中再開拒否の分類、ストリームのpad名再利用、字幕購読・時計・期限のテストも通過。

## 実機検証

表示サーバー、NVIDIA OpenGL、PulseAudioの動作を事前に確認し、新規プロセスで実施。
字幕・EPGを有効にし、停止・再開3回、番組表の開閉、字幕・EPGのOFF/ONを操作した。
番組表と映像の同時表示をスクリーンショットでも確認。EPGは15,341件を取得し、
OFF/ON後に再取得した。初回再生RSS 236.2MiB、操作後254.4MiB、停止後230.7MiB。
字幕購読はOFF時・停止後に0へ戻った。字幕受信はこの試験では0件であり、
実放送字幕の描画品質や表示同期を今回再検証できたという意味ではない。

最初のキーによる選局操作はPLAYING局IDが変わらなかったため、選局成功とは数えない。
追加の新規プロセスで「次」「前」ボタンを使用し、PLAYING局IDが
`3209641984 → 3209641985 → 3209641984` と変わることを確認した。
両プロセスともエラー・クラッシュなし、正常終了。

ログ、操作スクリプト、番組表画像はGit対象外の
`benchmark/refactor-typed-state/`、`benchmark/refactor-channels/`、
`benchmark/allocator-comparison/refactor*.py` と `refactor*results.jsonl` に保存。
既存の視聴プロセスは維持し、検証プロセスだけを正常終了した。

glibcの128KiB設定、映像パイプライン、停止時READY・終了時NULLの方針は変更していない。
長時間試験、実際の5分更新を何度もまたぐ視聴、mainの全機能移植は今回の検証範囲外。
