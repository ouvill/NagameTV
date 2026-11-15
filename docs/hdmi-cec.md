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

Flatpak版などのサンドボックス環境におけるデバイス公開範囲と権限要件は、パッケージ側の設定確定後に記載します（**TBD**）。

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

HDMI-CECから受け取ったキーは、既存のViewer操作経路（`player/remote.rs` の操作、ナビゲーション用の `remote_key_requested`、および数字・カラー入力用の `hdmi_cec_input_requested` → `ShortcutKey` 転送）を再利用して実行します。

下表は設計目標と契約に基づく対応表です。バックエンド実装の完了後、実際のCECユーザー制御コード（`CEC_OP_UI_CMD_*` 等）と長押しリピート条件の確定仕様を反映します（**TBD**）。

| テレビリモコンのキー | ながめTVでの動作 | 配送経路・備考（CECコード詳細はTBD） |
| --- | --- | --- |
| **上 / 下 / 左 / 右** | フォーカス移動・10秒シーク | `remote_key_requested` 経由。映像フォーカス時は[D-pad操作](shortcut-actions-design.md#視聴画面のd-pad操作)に従う |
| **決定（Select / OK）** | 決定（`Return` 相当） | `remote_key_requested` 経由 |
| **戻る（Back / Exit）** | 前面のパネル・画面を閉じる（`Back` 相当） | `remote_key_requested` 経由。全画面表示は維持する |
| **チャンネル ▲ / ▼** | 前 / 次のチャンネルへ選局 | 既存リモート操作経路を再利用（チャプター移動との分岐条件はTBD） |
| **数字 `0`〜`9`** | リモコン番号で選局 | `hdmi_cec_input_requested` 経由で既存キーボード選局と同じ2桁入力規則を使用 |
| **直前チャンネル（Previous Channel）** | 直前に見ていたチャンネルへ戻る | `L` キー相当の既存操作を使用 |
| **再生 / 一時停止 / 停止** | 再生 / 一時停止 / 停止 | 既存の再生操作と同じ条件 |
| **早送り / 巻き戻し** | 10秒進む / 10秒戻る（シーク） | 長押しリピート可否はTBD |
| **スキップ 前 / 次（Forward / Backward）** | 前 / 次のチャプターへ移動 | チャプターのある録画をシーク可能なとき |
| **カラーキー（青・赤・緑・黄 / F1〜F4）** | データ放送のカラーキー操作 | `hdmi_cec_input_requested` 経由でQtキーを配送（対象詳細はTBD） |

未対応のキーコードを受信した場合はデバッグログへ記録して無視します。

---

## 状態一覧とトラブルシューティング

`hdmi_cec_state` は次の8種類の固定文字列をとります。設定画面の「テレビのリモコン（HDMI-CEC）」欄には現在状態のほか、保持されたエラー詳細（`hdmi_cec_error`）と、転送されるボタンがテレビメーカーに依存する旨の注意書きを表示します。運用上の失敗は標準エラー出力（`tracing`）にも記録します。

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

## 関連ドキュメント

- [ショートカットとウィンドウ操作](shortcut-actions-design.md): 共通の操作体系とD-pad操作仕様
- [遠隔操作API](remote-control.md): LAN上の端末から操作するgRPC / gRPC-Web機能
- [開発・ビルド・診断](development.md): フィーチャーフラグ一覧と検証手順
