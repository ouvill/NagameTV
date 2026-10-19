# Webリモコンのバックエンド契約

M1で確定した追加契約。既存のRPCとフィールド番号は維持する。
実装と検証の進捗は `build/remote-web/report-backend.md` に記録する。
通信の型・enum値・入力範囲は
[`player.proto`](../rust/crates/viewer-remote/proto/viewer/v1/player.proto)を正とする。

## PlayerのQML公開API

既存の `configure_remote(enabled, address, port)` と `refresh_remote_addresses()` は維持する。
以下のプロパティは読み取り専用。起動後不変の `remote_web_allowed` はCONSTANTで、
その他は変更時に同名の `_changed` シグナルを通知する。

| 名前 | Qt型 | 意味 |
| --- | --- | --- |
| `remote_web_enabled` | bool | このプロセスでのWeb配信の希望設定。既定OFF。受付のON/OFFとは独立 |
| `remote_web_allowed` | bool | 起動時の機能許可リストでWeb配信の変更が許されているか。起動後不変 |
| `remote_web_urls` | QString | 接続先URLのJSON配列。例: `["http://192.168.1.10:50051/"]`。Web有効かつ受付中のときだけ列挙する |
| `remote_watchers` | int | 現在のSessionのWatchState購読数。端末数ではなく購読数。停止中は0。上限16 |

| invokable | 戻り値 | 意味 |
| --- | --- | --- |
| `set_remote_web_enabled(enabled: bool)` | bool | 許可されれば設定を変更しtrue。許可リストで禁止されたON要求はfalse。受付中は旧Sessionの終了後に再開。保存失敗は既存の `remote_save_error` へ通知 |
| `remote_qr_data_uri(url: QString)` | QString | `remote_web_urls` に含まれるURLのQRを `data:image/svg+xml;base64,...` で返す。不正・不在のURLは空文字列を返し、失敗をtracingへ記録 |

| シグナル | 意味 |
| --- | --- |
| `remote_key_requested(key: int)` | 検証済みのRemoteKeyをQtスレッドで通知。UP=1、DOWN=2、LEFT=3、RIGHT=4、OK=5、BACK=6、GUIDE=7、CHANNELS=8、FULLSCREEN=9 |
| `remote_command_executed(kind: int)` | 操作成功後に一度通知。値はprotoのRemoteCommandKind。QMLでトースト文言を翻訳する |

操作種類はSELECT_CHANNEL=1、PLAY=2、STOP=3、SET_VOLUME=4、SET_MUTED=5、
SET_SUBTITLES=6、SET_PAUSED=7、SEEK_RELATIVE=8、SEEK_TO=9、RETURN_TO_LIVE=10、
SET_PLAYBACK_RATE=11、STEP_CHANNEL=12、PREVIOUS_CHANNEL=13、SELECT_REMOTE_KEY=14、SEND_KEY=15。
0は未指定で、Rustは通知しない。失敗・取消し・期限切れの操作も通知しない。
`SendKey` の成功はQMLへのシグナル通知を意味し、画面の遷移やキー処理の完了は保証しない。

`ShortcutKey.post_key(item: QQuickItem*, key: int) -> bool` はQtの
Up/Down/Left/Right/Return/Backを受け付け、itemのwindowへpress/releaseを投稿する。
protoのRemoteKeyはQMLでQt.Key_*に変換する。修飾キーとautoRepeatは付けず、
ウィンドウをアクティブにする要求も行わない。未対応キーとwindow不在はfalse。
trueはイベント投稿の成功であり、フォーカス先の処理完了を意味しない。
専用Xwayland上のQt 6で確認したところ、非アクティブwindowには投稿できても
Keysへのpress/releaseは届かなかった。アクティブなwindowでは通常のフォーカス処理と
ViewerBackButtonによるBack変換を確認した。フォーカスを奪う処理は追加していない。
この挙動は `python3 scripts/test.py remote-keys` で検証する。

## RPCと状態

`SetPaused(paused)`、`SeekRelative(delta_ms)`、`SeekTo(position_ms)`、`ReturnToLive()`、
`SetPlaybackRate(tenths)`、`StepChannel(offset)`、`PreviousChannel()`、
`SelectRemoteKey(number, band)`、`SendKey(key)` を追加する。
速度は既存Playerと同じ0.5〜2.0倍、0.1倍刻み。`tenths=10` が等速。
シークの座標はPlayerStateのミリ秒値と共通で、ライブでは保持中のセッションの時間軸を使う。
範囲外の有効なシーク値は既存Playerと同じく保持範囲内へ丸める。
数値の欠落・定義域外・未知enumはINVALID_ARGUMENT、機能の未準備はFAILED_PRECONDITION。
局が存在しない場合はNOT_FOUND。その他の期限・キュー・エラー分類は[遠隔操作API](remote-control.md)を維持する。

PlayerStateに位置・長さ・シーク範囲・適用速度・操作可否・ライブ最新位置の情報を追加する。
不明な時間はoptionalの不在で表し、0と区別する。UIは `can_*` で操作を制御するが、
実行時にはRustが前提を再検査する。ListChannelsに各局の現在番組とリモコン番号を追加し、
番組の変更でも既存のrevisionを更新する。

## 設定とHTTP

`remote-control.toml` に `remote_web_enabled = false` を追加する。旧設定の欠落値はOFF。
通常起動では保存値を使い、`--features=...,remote-web` は既存の厳密な許可リストに従う。
`remote-web` を含む制限起動ではWebをONにし、設定は保存しない。含まない制限起動ではOFFに固定する。
リモート受付そのものは既存の設定で有効化する。

全HTTP要求に共通のHost検査を行う。IPv4リテラル、角括弧付きIPv6リテラル、
`localhost`（大文字小文字は区別しない）に任意の数値ポートを許可する。
HTTP/2ではURIの `:authority` を検査する。Hostとauthorityが両方あれば双方を検査する。
両方ない要求と不正な値、DNS名はHTTP 403で拒否しtracingへ記録する。
grpcurlの通常のIP/localhost接続はauthorityを送るため利用できる。

WebがOFFなら `/` と `/assets/*` は404。ONならバイナリーへ埋め込んだdistを返す。
index.htmlはno-cache、ハッシュ付きassetは長期キャッシュ。CORSは追加しない。
