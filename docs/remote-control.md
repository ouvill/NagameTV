# 遠隔操作API

起動中のViewerを操作する `viewer.v1.PlayerService`。
tonicでgRPC、tonic-webでgRPC-Webを同じポートへ公開する。Web UIは含めない。
家庭内LANの端末を信頼するリモコンとして、待受アドレスに接続できる端末からの
全RPCを認証なしで受け付ける。トークンの用意やペアリングは不要。
定義は[`player.proto`](../rust/crates/viewer-remote/proto/viewer/v1/player.proto)、
生成ドキュメントは[APIリファレンス](api/remote-control.md)。

## 設定と起動

設定画面の「リモート操作」で有効／無効を切り替える。初期値は次のとおり。

| 項目 | 初期値 |
| --- | --- |
| リモート操作 | OFF |
| 待受IPアドレス | `0.0.0.0`（すべてのIPv4インターフェース） |
| ポート | `50051` |

通常はONにするだけで使える。IPとポートの編集は「適用・再試行」またはEnterで反映する。
OFFにすると受付・購読を停止し、IPとポートは保存したままにする。
変更時は古い受付の終了を確認してから、新しいアドレス・ポートで開始する。
入力途中のIPやポートが不正でも、OFF操作は保存済みの設定を使って受付を停止する。

各Viewerプロセスが自身のAPIを持つ。複数を操作する場合は、追加のViewerのポートを
`50052`などへ変更する。ポートが使用中でもアプリは起動し、視聴は続けられる。
設定画面に受付の失敗を表示し、ポート変更または明示的な再試行で復旧する。
勝手に別ポートへ移動したり、他のViewerの終了後に自動で引き継いだりはしない。

設定画面の「接続先」には、待受IPが `0.0.0.0` なら実際のIPv4インターフェースの
アドレスを列挙する。ネットワーク変更後は設定を開き直すと再取得する。
LANアドレスがないときのloopback表示は同じPCからの接続用。
IP一覧は [`local-ip-address::list_afinet_netifas()`](https://docs.rs/local-ip-address/0.6.13/local_ip_address/)
で取得する。IP取得モジュールでは `unsafe` を禁止し、OSごとの取得処理はcrateへ任せる。
このAPIはインターフェースの稼働状態を返さないため、停止中のインターフェースに
残っているIPも表示候補に含まれる。到達性を確認した一覧ではない。
API自身は接続元が家庭内LANかを判定しない。通信は平文HTTP。
ブラウザーの別オリジンを許可するCORS設定はこの版には含めない。

設定は `~/.config/mirakurun-viewer/remote-control.toml` に保存する
（`XDG_CONFIG_HOME` に対応）。他の設定とはファイルを分け、別Viewerの音量変更や
終了時の保存でリモート設定が置き換わらないようにする。
複数Viewerの明示的な設定変更は最後の保存が次回起動の既定値になる。
すでに起動中の別Viewerの受付設定は変更しない。

```toml
enabled = false
address = "0.0.0.0"
port = 50051
```

起動時には環境変数で上書きできる。

| 環境変数 | 意味 |
| --- | --- |
| `MIRAKURUN_REMOTE_ENABLED` | `1` / `true`でON、`0` / `false`でOFF |
| `MIRAKURUN_REMOTE_ADDR` | 待受IP。例: `0.0.0.0`、`127.0.0.1`、`::1` |
| `MIRAKURUN_REMOTE_PORT` | `1`〜`65535`のポート |

未指定の項目は保存値を使う。IPやポートだけの指定ではONにしない。
上書きが1つでもある起動では、設定画面での変更も含めてそのプロセスだけに適用し、
ファイルへ書き戻さない。設定画面にも今回限りであることを表示する。

```sh
# 初期設定なら0.0.0.0:50051で受付を開始
MIRAKURUN_REMOTE_ENABLED=1 ./build/mirakurun-viewer
# 追加のViewerを別ポートで操作
MIRAKURUN_REMOTE_ENABLED=1 MIRAKURUN_REMOTE_PORT=50052 ./build/mirakurun-viewer
```

以前の `MIRAKURUN_REMOTE_ADDR=127.0.0.1:50051` という指定も受け付ける。
この形式は有効化も兼ねるが、`MIRAKURUN_REMOTE_ENABLED=0` があればOFFを優先する。
ポートを二重指定しないよう、この形式と `MIRAKURUN_REMOTE_PORT` の併用はエラーにする。
設定ファイルや環境変数の形式不備では受付を開始せず、設定画面に理由を表示する。
読めない設定ファイルを初期値で上書きせず、その起動中の変更は一時設定にする。

Flatpakでも同じ設定画面を使える。保存先はアプリ専用の設定領域にある
`mirakurun-viewer/remote-control.toml`。起動時の指定例:

```sh
flatpak run --env=MIRAKURUN_REMOTE_ENABLED=1 --env=MIRAKURUN_REMOTE_PORT=50052 io.github.ouvill.litv
```

APIは通常のデスクトップアプリとともに動作し、表示・GPU・音声要件は変わらない。

## 呼び出し

grpcurlではリポジトリーの定義を指定する。サーバーreflectionは公開していない。

```sh
grpcurl -plaintext \
  -import-path rust/crates/viewer-remote/proto -proto viewer/v1/player.proto \
  192.168.1.10:50051 viewer.v1.PlayerService/GetState
```

選局は `-d '{"channelId":"42"}'` と `viewer.v1.PlayerService/SelectChannel`、
音量変更は `-d '{"fraction":0.3}'` と `viewer.v1.PlayerService/SetVolume` を指定する。
64bit IDのProtobuf JSON表現は文字列。値は局一覧から取得し、表示順のindexは使わない。

将来のConnectクライアントでは `@connectrpc/connect-web` の
`createGrpcWebTransport({ baseUrl, useBinaryFormat: true })` を使う。
`createConnectTransport` のConnectプロトコルは扱わない。
UnaryとServer streamingを提供する。

## 操作と状態

- `ListChannels`: 現在の全局一覧。サブチャンネルを含む。
- `SelectChannel`: 現行カタログのIDを再検証して選局・再生する。IDは接続中のMirakurunに属する。
- `Play` / `Stop`: 選択局の再生／停止。ローカル録画を選択中はその録画の先頭からの再生／停止。停止は起動時の自動再生待ちも解除する。
- `SetVolume`: 0〜1の有限値。既存の音量スライダーと同じくミュートも解除する。
- `SetMuted`: ミュートの状態を明示する。選択した音量は維持する。
- `SetSubtitles`: 字幕表示を指定する。解析機能の許可は変更しない。同じ値の再送で字幕を消さない。
- `GetState` / `WatchState`: 選択局、ストリーム状態と対象局、音量、ミュート、字幕、現在番組、再生／設定保存エラー。

操作RPCはQtスレッドで操作を実行してから応答する。キューに入っただけでは成功を返さない。
再生開始は非同期なので、`connecting` から `playing` への遷移は状態通知で確認する。
操作後の状態を公開してから応答するため、その後の `GetState` はその操作以降の状態を返す。
通信スレッドはQtのプロパティを読み書きしない。

録画中の状態は`file_connecting`／`file_playing`／`file_stop_failed`で、表示用のファイル名を持つ。
フルパスは含まない。録画中の`current_program`は不在となる。ファイルを開く操作はデスクトップUIから行い、
APIのPlayは既に選択した録画を再生する。SelectChannelを呼ぶとライブ視聴へ切り替わる。

`WatchState` は接続直後に完全な状態を返し、以後は変更時に最新の状態を通知する。
局一覧の変更でもrevisionが増えるので、必要に応じて `ListChannels` を再取得する。
遅い受信者のために履歴を蓄積せず、中間状態は省略できる。
切断後はクライアントがバックオフ付きで再接続し、最初の状態で表示を復元する。
revisionはAPIの起動単位で有効。再起動をまたぐ比較には使わない。
API操作以外の変化は通常50msのPlayer pollで公開する。UI停止時の即時更新は保証しない。

| gRPC status | 意味 |
| --- | --- |
| INVALID_ARGUMENT | 必須値の欠落・不正な音量 |
| NOT_FOUND | 選局対象が現行カタログにない |
| FAILED_PRECONDITION | 再生出力・接続・選択局が未準備、字幕機能が無効 |
| RESOURCE_EXHAUSTED | 操作キューまたは購読数の上限 |
| DEADLINE_EXCEEDED | 2秒以内に操作結果が得られなかった |
| UNAVAILABLE | アプリ終了中・要求破棄 |
| INTERNAL | ネイティブの再生開始・停止失敗 |

応答を失った操作は実行済みの可能性がある。自動再送せず状態を読み直す。
未実行の要求は実行直前に切断・2秒の期限を検査して破棄する。
実行開始後の取消しで操作を巻き戻さない。
設定の保存時期は既存UIと同じ。RPC成功は保存成功を意味せず、失敗は `settings_error` に現れる。

## 所有と上限

`viewer-remote` はQt・GStreamerに依存しないcrate。
検証済み `Config` → ソケット所有者 `Bound` → `Session` → `Stopping` の順で資源を渡す。
要求も `Pending` → 期限確認済み `Executing` → 完了で消費し、同じ要求を二度実行できない。
既存のTokio runtimeを使う。操作キュー32件、Qtの1回のpollで最大8件、同時状態購読16件。
RPCのデコード上限4KiB、エンコード上限4MiB。

終了時は受付・購読を止め、未実行の操作を破棄して通信タスクを停止する。
停止待ちは最大3秒で、終了確認には `Stopping::poll` / `wait` を使う。
Qt engine破棄時に残ったタスクはabortし、所有元のNetwork runtime破棄で終了する。
再設定中の要求は旧Sessionとともに破棄し、新しいSessionへ移さない。

## 生成・検証

通常ビルドはtonic-prost-buildとCargo.lockで固定したprotoc-bin-vendoredを使う。
ホストへのprotocのインストールは不要。Flatpakのオフライン依存一覧にも含める。
ドキュメント生成には `buf 1.73.0`、`protoc-gen-doc 1.5.1` とPython 3をPATHへ用意する。
ローカルで生成し、スキーマを外部レジストリーへアップロードしない。

```sh
bash scripts/generate-remote-docs.sh
CARGO_TARGET_DIR=build/cargo cargo test --manifest-path rust/crates/viewer-remote/Cargo.toml --locked
```

通信試験はloopbackだけを使用し、表示・GPU・音声は使用しない。
変更後は本体のRust試験と `scripts/test-connection.sh`、機器検証付きの
`scripts/test-startup.sh` も実行する。
APIを含む比較元コミットができたら、
`buf breaking --against '.git#ref=比較元コミット'` でスキーマ互換性を検査する。
意味上の互換性はprotoコメントと操作テストで維持する。

### 検証記録（2026-09-15）

- 通信crateの8試験成功。認証情報なしのgRPCとHTTP/1.1のバイナリーgRPC-Web、入力検証、
  状態購読・再接続、上限、取消し・期限切れ、停止時の購読終了・ソケット解放を確認した。
- 本体のRust試験158件成功・既存の手動試験3件ignored。
  `test-connection.sh` では実PlayerへのRPC、音量・ミュート反映、未準備時の拒否、
  自動再生待ち解除とAPI終了を機器なしで確認した。設定画面への接続後は、
  2つのPlayerのポート競合・別ポートへの変更・連続再設定・OFF時のソケット解放、
  保存失敗と再試行、他の設定の保存がリモート設定を置き換えないことも確認した。
- 設定画面のQML試験23件成功。初期値、ON/OFF、不正入力、受付失敗、
  接続先と今回限りの設定表示を確認した。フォルダー選択の既存試験は、ダイアログを
  閉じた後に親画面の描画を待ってから次のキー入力を送るようにした。
  `test-localization.sh` の翻訳切り替え・カタログ欠落試験も成功。
- 初期実装ではX11・GPU・PulseAudioの検証後、通常起動したアプリのgRPC-Webへ接続し、
  局一覧取得・音量変更・選局、HTTP 503を返すローカル配信元での非同期再生失敗、停止を確認した。
  実放送の受信成功・Connectクライアント・Flatpakパッケージ再生成は今回の確認に含めない。
  認証なしのAPI動作は、機器不要の通信試験・実Player連携試験で確認した。
- Buf format/lint・ドキュメント生成、Clippy、CMakeリリースビルドが成功。
- `test-startup.sh` は認証撤去後の再実行で全項目成功。
  設定画面への接続後も全項目成功し、製品Main.qmlからの有効化、設定ページの表示、
  無効化とソケット解放を追加で確認した。
  初期実装の検証時には保存済み起動の局一覧待ちでタイムアウトし、変更前の
  `bdd5458` を別worktreeでビルドしても、同じ
  `!player.loading && root.channelRows.length === 2` の待機で失敗していた。
  原因は未特定で、今回の認証撤去によって修正されたとは扱わない。
- IP取得を `local-ip-address 0.6.13` へ置き換えた後、Linuxの実インターフェース一覧と
  製品の接続先整形を確認した。IPv4・IPv6のワイルドカードと明示IPを確認し、
  link-local IPv6を除外できている。Clippy、`test-connection.sh`、CMakeリリースビルド、
  書式検査とFlatpak依存一覧の整合性検査も成功。Windows・macOSでの実行確認は行っていない。

参考: [tonic-web](https://docs.rs/tonic-web/0.14.6/tonic_web/)、
[Connectのプロトコル選択](https://connectrpc.com/docs/web/choosing-a-protocol/)、
[protoc-gen-doc](https://github.com/pseudomuto/protoc-gen-doc)、
[Buf互換性検査](https://buf.build/docs/breaking/)。
