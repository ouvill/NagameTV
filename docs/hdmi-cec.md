# HDMI-CECリモコン操作（実験的）

テレビのリモコンを使って、HDMI経由でながめTVを操作する実験的機能です。LinuxカーネルのCECフレームワーク（`/dev/cecN`）からリモコンのキー入力を受信し、ながめTVの選局や再生操作へ割り当てます。

初期状態はOFFです。Phase 1では、自身をOSD名「`NagameTV`」のPlaybackデバイスとしてテレビ側へ通知し、リモコンからのキー受信のみを行います。テレビの電源連動（起動・スタンバイ）、入力自動切り替え（Active Source）、システムオーディオ制御はこの段階に含めません。

---

## 対応環境と前提条件

この機能は **Linux専用** です。LinuxカーネルのCECデバイス（`/dev/cec*`）を使用するため、Linux以外のOSでは状態が `unsupported` となり、`hdmi_cec_allowed` が `false` になって有効化できません。

### ハードウェアの要件

- **Raspberry Pi**: HDMI出力（`vc4-hdmi` ドライバー等）がカーネルのCECアダプターとして `/dev/cec0` 等を公開するため、外付けアダプターなしで利用できる場合があります。利用できるポートやデバイス番号は機種・接続ポート・カーネル設定に依存します。
- **USB CECアダプター**: Pulse-Eight USB-CECアダプターなど、Linuxカーネルが `/dev/cecN` として認識するアダプター（`pulse8-cec` 等）を利用できます。
- **一般的なPCのGPU**: デスクトップPCやノートPC内蔵のHDMI出力（Intel / AMD / NVIDIA製GPUの多く）にはCEC信号線が実装されていないため、`/dev/cec*` が現れません。通常のPCで利用する場合は、USB CECアダプターを併用してください。

### デバイス権限（`video` グループ等）

`/dev/cec*` は多くのディストリビューションで `video` グループ所有（例: `crw-rw---- root video`）として作成されます。アプリを実行するユーザーに読み書き権限が必要です。

```sh
# 現在のデバイスと権限を確認
ls -l /dev/cec*

# ユーザーが video グループに含まれているか確認
groups
```

権限がない場合は、ユーザーを `video` グループへ追加してから再ログインするか、udevルールで適切なアクセス権を設定してください。

```sh
sudo usermod -aG video "$USER"
```

### 配布形式ごとのデバイス公開と権限

- **Flatpak版**: 現行のマニフェストは `--device=dri` のみを許可しており、`/dev/cec*` を公開していません。そのため、現行のパッケージ版 Flatpak では HDMI-CEC を利用できません。今後対応するには、パッケージ側で必要なデバイス権限（例: `--device=all` 等の付与）を別途判断する必要があり、有効化した場合もホスト側の UNIX デバイス権限（`video` グループ等）が適用されます。
- **AppImage版 / deb版**: アプリ固有のデバイスサンドボックスを持たないため、ホスト側の `/dev/cecN` の権限（通常は `video` グループ）をそのまま利用します。`libcec` 等の外部ネイティブパッケージの追加導入は不要です（Linux カーネルの CEC uAPI `ioctl` を直接使用します）。

---

## 有効化と設定

### 設定の保存と起動フラグ

- **実行時フラグ**: `hdmi_cec_enabled`（既定 `false`）。OFFのときは `/dev/cec*` を開かず、監視スレッドも起動しません。実行中にOFFへ切り替えると、戻る前にワーカーとデバイス（および論理アドレス）を同期的に解放します。
- **設定の保存**: 通常起動では既存の設定保存先（`settings.toml`）に `hdmi_cec_enabled` と `hdmi_cec_device` を保存・復元します。`hdmi_cec_device` が空文字列 `""` のときは自動選択（最初に利用可能な `/dev/cecN`）を使い、明示指定するときは `/dev/cecN` 形式のパスを指定します。
- **設定画面からの有効化**: 「設定 → リモート操作」の「テレビのリモコン（HDMI-CEC）」セクションにある「HDMI-CECでテレビのリモコンから操作する（実験的）」をONにします。ONにすると、使用するアダプターを選ぶ「CECデバイス」（「自動」または「`<名前> (/dev/cecN)`」）と「再検索」ボタンが表示されます。
- **検証用の起動オプション**: `--features=hdmi-cec`（他機能と併用可。例: `--features=epg,subtitles,hdmi-cec`）。`hdmi-cec` を指定した制限起動では一時セッションとして有効になり、設定ファイルへ書き戻しません。`hdmi-cec` を含まない `--features` 指定では `hdmi_cec_allowed` が `false` となり、設定画面からも有効化できません。
- **再検出と自動再試行**: 有効化時には自動で `/dev/cec*` を走査します。設定画面の「再検索」ボタンを押すと `refresh_hdmi_cec_devices()` で再走査・再試行します（OFFのときはデバイスを開かずワーカーも作りません）。`no-device` と `disconnected` の状態では、有効な間は定期的に再試行します。

### QML公開API（`Player`）

`Player` が以下の読み取り専用プロパティ、invokable、シグナルを公開します。変更系の操作後は `hdmi_cec_changed()` が選択中デバイスを含む一貫したスナップショットを通知します。

| 種別 | 名前・型 | 内容 |
| --- | --- | --- |
| プロパティ | `hdmi_cec_enabled: bool` | 有効な実行時設定。既定 `false` |
| プロパティ（CONSTANT） | `hdmi_cec_allowed: bool` | Linux環境かつ起動時許可リストで有効化が許可されているか |
| プロパティ | `hdmi_cec_state: string` | 現在の状態（後述の8状態） |
| プロパティ | `hdmi_cec_error: string` | 保持されているエラーメッセージ。正常時は空文字列 |
| プロパティ（CONSTANT） | `hdmi_cec_devices: HdmiCecDeviceModel*` | デバイス一覧の `QAbstractListModel`（`count`、roleは `path`・`name`・`driver`） |
| プロパティ | `hdmi_cec_device: string` | 選択中のデバイスパス。空文字列は自動選択 |
| プロパティ（CONSTANT） | `hdmi_cec_osd_name: string` | 固定値 `NagameTV` |
| プロパティ | `hdmi_cec_physical_address: string` | 判明している場合のドット区切り物理アドレス（例: `1.0.0.0`）。不明時は空文字列 |
| invokable | `set_hdmi_cec_enabled(enabled: bool) -> bool` | 有効／無効を切り替え、許可された変更を保存する。禁止時は `false` を返す。OFF時は完了前にデバイスを解放する |
| invokable | `select_hdmi_cec_device(path: string) -> bool` | `""`（自動選択）または `/dev/cecN` を受け付け、保存する。有効中に変更するとセッションを再起動する。不正値は `false` |
| invokable | `refresh_hdmi_cec_devices()` | 有効時にデバイスの再走査・再試行を行う。OFF時は何もしない |
| シグナル | `hdmi_cec_changed()` | 可変プロパティが更新されたときの共通通知 |
| シグナル | `remote_key_requested(key: int)` | 既存の `RemoteKey` enumでナビゲーションキーを通知 |
| シグナル | `hdmi_cec_input_requested(key: int, repeat: bool)` | 数字・カラーキー入力をQtキーコードとして通知し、既存の `ShortcutKey` 転送経路へ渡す |

---

## キー割り当て

HDMI-CECから受け取ったユーザー制御コード（`0x00`〜`0x74`）は、既存のViewer操作経路を再利用して実行します。ナビゲーション操作は `remote_key_requested`、数字・シーク・チャプター・データ放送カラーキーは `hdmi_cec_input_requested` 経由で QML の `ShortcutBindings.dispatch`（および表示中のデータ放送カラーキー処理）へ渡し、選局・再生制御は `player/remote.rs` の操作へ接続します。

| テレビリモコンのキー | CECコード | ながめTVでの動作 | 配送経路・備考 |
| --- | --- | --- | --- |
| **決定（Select / Enter）** | `0x00` / `0x2b` | 決定（`RemoteKey::Ok`） | `remote_key_requested` 経由 |
| **上 / 下 / 左 / 右** | `0x01` / `0x02` / `0x03` / `0x04` | 上下左右の移動 | `remote_key_requested` 経由。映像フォーカス時は[D-pad操作](shortcut-actions-design.md#視聴画面のd-pad操作)に従う |
| **戻る / 終了（Back / Exit）** | `0x0d` | 前面のパネル・画面を閉じる（`RemoteKey::Back`） | `remote_key_requested` 経由。全画面表示は維持する |
| **チャンネル ▲ / ▼** | `0x30` / `0x31` | 次 / 前のチャンネルへ選局 | `Command::StepChannel`（`0x30` が次の局、`0x31` が前の局）。録画再生中もチャプター移動ではなくチャンネル操作として扱う |
| **数字 `0`〜`9`** | `0x20`〜`0x29` | リモコン番号で選局 | `hdmi_cec_input_requested`（`Qt.Key_0`〜`9`）経由で `ShortcutBindings.dispatch` へ渡し、キーボードと同じ2桁入力（10〜12）規則を使う |
| **直前チャンネル** | `0x32` | 直前に見ていたチャンネルへ戻る | `Command::PreviousChannel`（`L` キー相当） |
| **再生** | `0x44` / `0x60` | 再生 | `Command::Play` |
| **一時停止** | `0x46` / `0x61` | 一時停止 | `Command::SetPaused(true)` |
| **停止** | `0x45` / `0x64` | 停止 | `Command::Stop` |
| **巻き戻し / 早送り** | `0x48` / `0x49` | 10秒戻る / 10秒進む（シーク） | `hdmi_cec_input_requested`（`Left` / `Right`）経由。**長押しリピート対象** |
| **スキップ 前 / 次** | `0x4c` / `0x4b` | 前 / 次のチャプターへ移動 | `hdmi_cec_input_requested`（`MediaPrevious` / `MediaNext`）経由。チャプターのある録画をシーク可能なとき |
| **カラーキー（青・赤・緑・黄）** | `0x71` / `0x72` / `0x73` / `0x74` | データ放送のカラーキー操作 | `hdmi_cec_input_requested`（`Qt.Key_Blue` / `Key_Red` / `Key_Green` / `Key_Yellow`）経由。データ放送表示中のみ `"b"` / `"r"` / `"g"` / `"y"` として処理 |

### 長押しリピートと入力の有効期限

- **長押しリピートの対象**: 連続入力（`repeat = true`）を許可するのは **巻き戻し（`0x48`）と早送り（`0x49`）のみ** です。その他のキー（方向・決定・戻る・選局・数字・再生/一時停止/停止・チャプター・カラーキー）は長押しリピートを抑止します。
- **押下状態のタイムアウト**: 送信元（initiator 0〜15）ごとに押下と解放を追跡します。解放通知がバス上で欠落した場合は、押下が途切れてから **550 ms** 経過で現在の押下状態を失効させ、次の押下を新しい操作として扱います。
- **世代管理と未知コード**: デバイスの切断や再設定によってセッションの世代が切り替わると、キューに残っていた古い入力は破棄して実行しません。未対応のキーコード・メッセージ・イベントはデバッグログへ記録して無視します。

---

## 状態一覧とトラブルシューティング

`hdmi_cec_state` は次の8種類の固定文字列をとります。設定画面の「テレビのリモコン（HDMI-CEC）」欄には現在状態のほか、保持されたエラー詳細（`hdmi_cec_error`）と、転送されるボタンがテレビメーカーに依存する旨の注意書きを表示します。

運用上の失敗は標準エラー出力（`tracing`）にも記録します。状態表示用の変換ではログを出さず、ワーカーが保持する診断履歴で同じ失敗の連続出力を抑えています。新しい失敗や原因・パス・I/O種別の変化、および `active` への復帰後に再び起きた失敗は `WARN` で一度記録し、同じ失敗が定期再試行（受信から再接続への移行を含む）や待機・解放処理で続く間は `DEBUG` に留めます。入力キューの溢れも投入成功まで一度だけ `WARN` を出し、カーネルからの新規イベント／メッセージ欠落通知は発生ごとに `WARN` で記録します。

| `hdmi_cec_state` | 意味 | 確認・対処方法 |
| --- | --- | --- |
| `disabled` | 設定OFF、または起動ポリシーで無効 | 「設定 → リモート操作 → テレビのリモコン（HDMI-CEC）」で有効にするか、`--features` に `hdmi-cec` を含める |
| `no-device` | `/dev/cec*` が見つからない（有効時は定期的に再走査） | `ls -l /dev/cec*` を確認する。PC内蔵GPUの多くはCEC非対応のため、USB CECアダプターを接続し「再検索」を試す。Raspberry Piではカーネル設定や接続するHDMIポートを確認する |
| `permission-denied` | `/dev/cec*` を開く権限がない | `groups` で `video` グループ所属を確認し、未所属なら `sudo usermod -aG video "$USER"` の後に再ログインする（またはudevルールを確認する） |
| `waiting-for-tv` | テレビ電源OFF、ケーブル未接続、または物理アドレスが無効 | テレビの電源を入れ、該当のHDMI入力へ切り替える。テレビ側のHDMI連動機能（CEC）がONか確認する。有効な物理アドレスが割り当てられると、カーネルの状態変化イベントを受けて論理アドレスの取得へ進む |
| `active` | 論理アドレスを取得し、OSD名 `NagameTV` でキー入力を待機中（UIでは物理アドレス `x.x.x.x` と `NagameTV` を表示） | — |
| `disconnected` | 使用中のデバイスが切断された（有効時は定期的に再試行） | USBケーブルやアダプターの接続を確認する |
| `error` | 論理アドレスの取得失敗などのエラー | `hdmi_cec_error` の表示と標準エラー出力の `tracing` ログを確認する |
| `unsupported` | Linux以外のOSで実行している | Linux環境で利用する |

---

## テレビメーカー別の制限事項

HDMI-CECでは、どのリモコンキーを外部機器（Playbackデバイス）へ転送するかがテレビ側の機種・設定・接続状態に依存します。

テレビ側のHDMI連動機能がONになっており、かつテレビの現在の入力がそのHDMIポートに切り替わっている必要があります。また、各社の連動機能では、一部のキーをテレビ本体が消費して外部機器へ送らないことがあります。

- **ソニー（BRAVIA Sync / ブラビアリンク）**: 機種や設定により方向・決定・戻る・再生系のキーは転送されますが、数字キー、カラーキー、番組表や音量系のキーはテレビ側で処理され届かない場合があります。
- **パナソニック（VIERA Link / ビエラリンク）**: ビエラリンクの設定や操作対象の選択状態によっては、再生系や方向キー以外のボタンがテレビ本体の操作として扱われることがあります。
- **シャープ（AQUOS Familink / ファミリンク）**: ファミリンクが有効でも、Playbackデバイスへ転送されるキーが方向・決定・再生操作などに限定される機種があります。
- **TVS REGZA / 東芝（REGZA Link / レグザリンク）**: レグザリンクで外部機器操作を有効にしても、チャンネル上下・数字・カラーキーはテレビ本体の選局が優先され、転送されない場合があります。

テレビの音量キーは、通常テレビ本体やAVアンプの音量調整に使われるため、アプリ側へ転送されないのが一般的です。リモコンのすべてのボタンが届くわけではないため、届かない操作はキーボード・マウスや[遠隔操作API](remote-control.md)と併用してください。

---

## 今後の拡張予定

Phase 1の検証と並行し、以下を今後の検討事項として残します。

- **電源連動**: 起動時のテレビ電源ON（One Touch Play）や、アプリ終了時のスタンバイ連動
- **入力切り替え**: 再生開始時などにテレビの入力を自動で切り替えるActive Source通知
- **音声連動**: テレビやAVアンプと音量・ミュートを連携させるシステムオーディオ制御（System Audio Control）

---

## 実装構成と検証状況

- **Rustモジュール構成**: コアの状態管理・デバイス抽象・ワーカー制御は [`rust/src/hdmi_cec/mod.rs`](../rust/src/hdmi_cec/mod.rs)、Linux カーネル CEC uAPI の `ioctl` 呼び出し（受信専用）は [`rust/src/hdmi_cec/linux.rs`](../rust/src/hdmi_cec/linux.rs)、キー割り当てと長押し抑止は [`rust/src/hdmi_cec/keys.rs`](../rust/src/hdmi_cec/keys.rs)、再試行時のログ重複抑制と診断履歴は [`rust/src/hdmi_cec/diagnostics.rs`](../rust/src/hdmi_cec/diagnostics.rs)、`Player` への橋渡しと一覧モデルは [`rust/src/player/hdmi_cec.rs`](../rust/src/player/hdmi_cec.rs)・[`rust/src/hdmi_cec_model.rs`](../rust/src/hdmi_cec_model.rs) が担当します。
- **送信操作の不在**: 外部クレート（`libcec` や `cec-linux` 等）を追加せず、Linux の `cec.h` 互換 `ioctl` を直接利用して再生側受信のみを実装しています。論理アドレスの告知と OSD 名 `NagameTV` の設定はカーネルが処理します。

### 実機検証が必要な項目

フェイクデバイスを用いた単体試験・Qt 接続試験・QML 部品試験は通過していますが、開発コンテナーには `/dev/cec*` が存在しないため、**実機での確認は未実施** です。実験扱いを解除する前に、Raspberry Pi（`vc4-hdmi`）または Linux 対応 USB CEC アダプターと実テレビで以下を確認する必要があります。

1. **デバイス検出・権限・共存**: 能力検出、自動／明示選択、実際の `video` グループ権限での開閉、他の CEC アプリが稼働中の環境で排他取得が既存アドレスを置き換えずに失敗すること。
2. **アドレス調停と待機復帰**: Playback 論理アドレスの取得、テレビ側での「`NagameTV`」表示、正しい物理アドレスの取得、テレビ電源 OFF／HDMI 未接続からの `CEC_EVENT_STATE_CHANGE` による自動復帰（物理アドレスをユーザー空間から設定する必要があるアダプターには非対応）。
3. **実テレビでのキー操作**: ナビゲーション、数字（10〜12 入力を含む）、チャンネル／直前チャンネル、再生／シーク／チャプター、データ放送カラーキーの各メーカーでの転送状況、長押しリピートおよび 550 ms タイムアウトの挙動。
4. **解放と再接続**: 実行中 OFF およびアプリ終了時に論理アドレス解放とファイル記述子クローズが行われること、USB 抜き差しから復旧すること、再接続時に切断前の古い入力が発火しないこと。

---

## 関連ドキュメント

- [ショートカットとウィンドウ操作](shortcut-actions-design.md): 共通の操作体系とD-pad操作仕様
- [遠隔操作API](remote-control.md): LAN上の端末から操作するgRPC / gRPC-Web機能
- [開発・ビルド・診断](development.md): フィーチャーフラグ一覧と検証手順
