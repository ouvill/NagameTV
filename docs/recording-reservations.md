# 番組表からの録画予約（実験的）

EPGStationへ番組単位の手動予約を追加・取消しする。機能は既定OFFとし、
Qtに依存しないRustの所有者がフラグ、接続世代、非同期操作と予約状態を管理する。
QMLは以下の契約で設定スイッチ、番組表の印、番組詳細の操作を接続する。

## QML公開API

### Player

| 種別 | 名前・型 | 契約 |
| --- | --- | --- |
| 読取専用プロパティ | `recording_reservations_enabled: bool` | 設定フラグ。既定false。接続前も設定できる |
| 読取専用プロパティ | `recording_reservations_allowed: bool` | 起動ポリシーが有効化を許す。CONSTANT |
| invokable | `configure_recording_reservations(enabled: bool)` | フラグを変更・保存する。allowed=falseでの有効化は拒否 |
| シグナル | `recording_reservations_enabled_changed()` | フラグのNOTIFY。操作の無効化を反映してから通知 |
| 読取専用プロパティ | `reservations_enabled: bool` | フラグONかつEPGStationの現在の認証セッションが接続確認済み |
| 読取専用プロパティ | `reservation_busy: bool` | 一覧取得・追加・取消し・その後の再取得が進行中。取消し待ちを含む |
| 読取専用プロパティ | `reservation_error: string` | 正常時は空。Backend文脈で翻訳済みの利用者向けエラー。URL・サーバー本文・資格情報を含めない |
| シグナル | `reservations_changed()` | 上記3プロパティと番組ごとの操作可否・予約状態の変更。通知前にモデルへ反映 |
| invokable | `refresh_reservations(): bool` | 一覧を再取得。未接続・フラグOFF・操作中はfalse |
| invokable | `reservation_state(program_key: string): string` | 番組の予約状態。値は下記roleと同じ。古いキー・曖昧な番組は空 |
| invokable | `can_reserve_program(program_key: string): bool` | 現在の状態で追加できるか。実行時にも検証する |
| invokable | `can_cancel_reservation(program_key: string): bool` | 現在の状態で取消しできるか。実行時にも検証する |
| invokable | `reserve_program(program_key: string): bool` | 手動予約の追加を受け付けたらtrue。非同期の完了・失敗はbusy/errorとreservations_changedで通知 |
| invokable | `cancel_reservation(program_key: string): bool` | 手動予約の取消しを受け付けたらtrue。同上 |

program_keyは`watch_program(key)`と同じ`watchKey`。QMLで番組IDへ分解しない。
二重操作は受け付けず、成功応答後も一覧再取得が終わるまでbusyを維持する。
通信失敗後は書込を自動再送しない。再取得して実際の状態を確認してから次の操作を行う。

### GuideModel

`Player.guide_model`へ`reservation: string` roleを追加する。
値は`""`（未予約・未取得・対象外）、`reserved`、`conflict`、`skipped`、`recording`。
重複予約はskippedに含め、同じ番組に複数状態がある場合は
recording → conflict → reserved → skippedの順に表示する。
番組表の競合セル・終了時刻不明のセルは、単一番組を特定できないため空を返す。

予約だけの変更では、変わった行のreservation roleに限定してdataChangedを通知する。
番組表のreset、EPGの再取得、本文コピー、汎用revisionの更新は行わない。
番組詳細の状態はPlayer.reservations_changedを受けてreservation_stateと操作可否を読み直す。

## 設定と所有関係

設定名は`recording_reservations_enabled`。既存設定と同じ保存・復元処理を使う。
検証用の`--features=epg,recording-reservations`では通常設定を読み書きしない。
`--features=none`など予約を含まない明示指定では設定からも有効化できない。

OFFでは予約専用Job・通信を開始しない。OFFまたは接続先・認証の変更時は
一覧を破棄し、進行中のJobを取消し、終了確認後に新しい要求を許す。
送信済みの追加・取消しはサーバーで完了している可能性があるため、ONへの復帰時は
一覧を取得し直す。フラグ切替でサーバー上の予約を取り消すことはない。

ONかつ確認済み接続では初回取得し、以後30秒間隔と明示更新・操作完了後に再取得する。
同時操作は1件。1応答4 MiB、一覧は合計10,000件、ページは100件に制限する。
上限超過や途中の取得失敗では、不完全な一覧を確定せず操作を無効化する。

## APIと番組IDの対応

調査対象は[EPGStationの固定コミット](epgstation.md#apiと所有関係)。
[ProgramDBのMirakurun番組変換](https://github.com/stuayu/EPGStation/blob/26f56720722e61e66fc33fc0c4aacfd3d98913ba/src/model/db/ProgramDB.ts)は
Mirakurunのprogram.idをProgram.idへそのまま保存する。
[手動予約処理](https://github.com/stuayu/EPGStation/blob/26f56720722e61e66fc33fc0c4aacfd3d98913ba/src/model/operator/reservation/ReservationManageModel.ts)は
このIDで番組を検索し、予約のprogramIdへ保存する。番組IDを局IDなどから再計算しない。

追加前に`GET /api/schedules/detail/{programId}?isHalfWidth=false`で
ID・局ID・開始／終了時刻を照合する。番組が存在しない場合や時刻が一致しない場合は
追加しない。異なるMirakurunや古いEPGを使う構成で別の番組へ予約することを避ける。

- `GET /api/reserves?type=all&isHalfWidth=false&offset=…&limit=100`で予約一覧を取得する。
- `GET /api/recording?isHalfWidth=false&offset=…&limit=100`の録画中一覧をprogramId・局・開始時刻で照合する。
- `POST /api/reserves`へ`{programId, allowEndLack:false}`を渡す。保存先・エンコードはサーバー既定値を使う。
- `DELETE /api/reserves/{reserveId}`で手動予約を取り消す。直前に予約詳細と録画中一覧を再確認する。

参照: [予約一覧・追加](https://github.com/stuayu/EPGStation/blob/26f56720722e61e66fc33fc0c4aacfd3d98913ba/src/model/service/api/reserves.ts)、
[予約詳細・取消し](https://github.com/stuayu/EPGStation/blob/26f56720722e61e66fc33fc0c4aacfd3d98913ba/src/model/service/api/reserves/%7BreserveId%7D.ts)、
[録画中一覧](https://github.com/stuayu/EPGStation/blob/26f56720722e61e66fc33fc0c4aacfd3d98913ba/src/model/service/api/recording.ts)。
通信は録画一覧と同じCookieセッションを使い、リダイレクトへ追従しない。

## 今回の対応範囲

既知の終了時刻が現在より後にある単一番組へ追加する。
取消しは開始前で録画中でない手動予約1件に限定する。ルール予約・時刻指定予約・
複数予約は状態表示のみとし、変更はEPGStationで行う。録画停止・ルール編集・
スキップ解除・予約詳細設定は今回の操作に含めない。
フラグの実験扱いを解除する条件と検証結果は[開発手順](development.md#トランクベース開発とフィーチャーフラグ)へ記録する。
