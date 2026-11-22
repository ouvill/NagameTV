# HDMI-CEC処理の見直し（カーネルのリモコン変換へ移行）

2026-11-22の実機調査に基づく移行計画です。ワーカー方式からカーネルへの論理アドレス設定に
置き換えました。利用手順は[HDMI-CECリモコン操作](hdmi-cec.md)を参照してください。

移行前のながめTVは、`/dev/cecN` を排他で開き、テレビとのやり取りとリモコンキーの解釈を
アプリ内で行っていました。移行後はLinuxカーネルがCECメッセージを処理し、アプリは論理アドレスを
設定して、カーネルが変換したキー入力を通常のキーボード入力として受け取ります。

---

## 背景と課題

- **排他の行儀**: 従来の実装は `CEC_MODE_EXCL_INITIATOR | CEC_MODE_EXCL_FOLLOWER` でデバイスを開いていました。
  アプリの実行中は、他のプロセスが送信できませんでした（`cec-ctl` による診断用の送信も含む）。
  アダプターの論理アドレスをユーザーセッションのアプリが占有していました。
- **機器依存と検証の難しさ**: ルーティング応答、`ACTIVE_SOURCE`、メニュー状態、キーの押下追跡を
  独自に実装していました。テレビメーカーごとに挙動が異なりますが、実機で確認できる環境は限られています。
- **維持コスト**: `rust/src/hdmi_cec/` の約2,700行に加えて、ワーカー、状態遷移、
  CEC専用のキー配送経路、設定UIを維持する必要がありました。
- **snapの権限**: 現在は `custom-device` で `/dev/cecN` を公開しているため、Storeで配布するには
  snap-declarationが必要です。

## 実機で確認したこと

確認環境は次のとおりです。

- **機器**: Raspberry Pi 4（[Piキオスク構成](pi-appliance.md)）とLG 43UN8100PJA。HDMI0に接続し、物理アドレスは `2.0.0.0`
- **カーネル**: `7.0.0-1023-raspi`

カーネルは `CONFIG_MEDIA_CEC_RC=y` でビルドされていました。`vc4-hdmi` アダプターは
`CEC_CAP_RC` を持ち、入力デバイス `vc4-hdmi-0`（`/dev/input/eventN`）を常に作成します。
`cec-ctl -d /dev/cec0 --playback --osd-name NagameTV` で `CEC_LOG_ADDRS_FL_ALLOW_RC_PASSTHRU`
付きの論理アドレスを設定すると、アプリやデーモンを動かさなくても、リモコンキーが入力イベントとして届きました。

| リモコンのキー | CECコード | 入力イベント |
| --- | --- | --- |
| 上 / 下 / 左 / 右 | `0x01`〜`0x04` | `KEY_UP` / `KEY_DOWN` / `KEY_LEFT` / `KEY_RIGHT` |
| 決定 | `0x00` | `KEY_OK` |
| 戻る | `0x0d` | `KEY_EXIT` |
| 青 / 赤 / 緑 / 黄 | `0x71`〜`0x74` | `KEY_BLUE` / `KEY_RED` / `KEY_GREEN` / `KEY_YELLOW` |
| 数字 | — | 届かない。押すとテレビがHDMI入力から自身のチューナーへ切り替わる |
| チャンネル▲▼ | — | 届かない |

- **長押し**: テレビは0.25秒ごとに押下と解放を繰り返して送るため、カーネルの自動リピートは発生しませんでした。
- **状態変化への追従**: 次の操作の後もキーは届き続けました。
  - テレビの入力切り替え
  - テレビの電源の入れ直し
  - HDMIケーブルの抜き差し

  HPDが切れて戻ると、カーネルは保持している設定で論理アドレスを自動的に取り直し、
  物理アドレス・OSD名・ベンダーIDの問い合わせに応答しました。
- **応答しなかった問い合わせ**: テレビは電源状態を繰り返し問い合わせ（`GIVE_DEVICE_POWER_STATUS`）、
  カーネルは `FEATURE_ABORT` を返しました。それでもキーの転送は止まりませんでした。
- **`ACTIVE_SOURCE` は不要だった**: このテレビでは、Pi側が `ACTIVE_SOURCE` を送らなくてもキーが転送されました。

## 役割分担

| 担当 | 内容 |
| --- | --- |
| カーネル | 論理アドレスの調停とHPD後の再取得、標準の問い合わせへの応答、リモコンキーの入力イベント化 |
| アプリ | CEC有効時に論理アドレスを1回設定し、自分で設定したときだけ解放する。キー入力の正規化。任意で `ACTIVE_SOURCE` を1回送信 |
| OS（任意） | キオスクなど、OSごと配布する構成では、起動時に論理アドレスを設定してもよい |

カーネルは論理アドレスを自動では設定しません。全ての配布形式で利用者の追加設定を不要にするには、
アプリが `CEC_ADAP_S_LOG_ADDRS` を呼ぶ必要があります。これは `cec-ctl` と同じ標準の
カーネル uAPI で、テレビの機種には依存しません。

アプリは監視スレッドを持たず、デバイスを排他にも開きません。キーはウィンドウにフォーカスがあるときだけ届きますが、
この制約は許容します。

## 論理アドレスの設定と所有

### 設定する内容

ワーカー方式で使っていた識別情報に、`CEC_LOG_ADDRS_FL_ALLOW_RC_PASSTHRU` を加えます。

| 項目 | 値 |
| --- | --- |
| OSD名 | `NagameTV` |
| デバイス種別 | Playback（論理アドレス種別も Playback） |
| ベンダーID | `0x000c03`（`cec-ctl` の既定値と同じ） |
| CECバージョン | 1.4 |
| フラグ | `CEC_LOG_ADDRS_FL_ALLOW_RC_PASSTHRU` |

### 有効化時の処理

1. `/dev/cecN` を自動選択または明示選択し、排他にせず開きます。
2. 能力を確認します。`CEC_CAP_LOG_ADDRS` と `CEC_CAP_RC` が必要です。
   `CEC_CAP_RC` がなければ、キーを受け取れない状態として表示します。
3. `CEC_ADAP_G_LOG_ADDRS` で現在の設定を読み、次の表に従って処理します。
4. fdを閉じます。設定はカーネルに残ります。

| 現在の設定 | 所有の印 | 処理 |
| --- | --- | --- |
| 未設定 | — | 上記の内容で設定し、所有の印を作る |
| ながめTVの識別情報と一致 | あり（前回のクラッシュの残り） | 引き継ぐ |
| ながめTVの識別情報と一致 | なし | 外部の設定として扱い、触らない |
| その他 | — | 外部の設定として扱い、触らない。パススルーフラグがなければ、キーが届かない可能性を表示する |

物理アドレスが無効な場合、`__cec_s_log_addrs()` は論理アドレスの設定を保持します。
その後、`__cec_s_phys_addr()` が有効な物理アドレスを設定すると、カーネルが取得を始めます。
すでに有効な場合、ブロッキングモードでは取得完了を待ちます。ながめTVは `O_NONBLOCK` と
既定の非排他 initiator モードを使います。成功は設定の受付を表します。取得前やHPD切断中も、
`CEC_ADAP_G_LOG_ADDRS` の `num_log_addrs` は要求数を返すため、この値で設定の有無を判定します。
[カーネルの設定と取得](https://github.com/torvalds/linux/blob/v7.0/drivers/media/cec/core/cec-adap.c#L1928-L1944)
[取得を始める物理アドレス更新](https://github.com/torvalds/linux/blob/v7.0/drivers/media/cec/core/cec-adap.c#L1697-L1730)
[設定を取得するioctl](https://github.com/torvalds/linux/blob/v7.0/drivers/media/cec/core/cec-api.c#L144-L194)
[Linux CEC uAPI](https://docs.kernel.org/userspace-api/media/cec/cec-ioc-adap-g-log-addrs.html)

### 所有の印

論理アドレスの設定はカーネルに残るため、外部の設定と、自分が過去に設定したものを区別する必要があります。
キオスクのOS設定も同じOSD名を使うので、カーネルの情報だけでは区別できません。

- 設定に成功したら、`$XDG_RUNTIME_DIR/nagametv/hdmi-cec/` にアダプターごとの印を作ります。
  `cecN.owner` にアダプター名とデバイスパスを記録します。同じ実行時ディレクトリのプロセス間では
  `cecN.lock` を使い、実行中の所有者からの引き継ぎを防ぎます。
- `$XDG_RUNTIME_DIR` とカーネルの設定は再起動で消えます。snapでは、snapごとの
  実行時ディレクトリを使います。
- `$XDG_RUNTIME_DIR` がない環境では、設定はしますが所有の印を作れません。
  この場合は終了時に解放できないことをログに記録します。

### 解放

次のときに、印があり、かつ現在の設定がながめTVの識別情報と一致する場合だけ、
`CEC_ADAP_S_LOG_ADDRS` を空の内容で呼んで解放します。その後、印を削除します。

- 通常の終了
- 実行中にOFFへ切り替えたとき
- 別のデバイスを選び直したとき

設定が別の内容に置き換わっていたときは解放せず、印だけを削除します。

カーネルが論理アドレスを取得中など、解放要求が `EBUSY` で失敗したときは、ログと状態表示に
記録し、次に解放または引き継ぎができるように印を保持します。CEC uAPIの読み取りと書き込みは
別々の操作です。同じ実行時ディレクトリを使わない外部ツールと同時に書き込む操作や、外部ツールが
同じ識別情報で設定を書き直す操作は、印と現在の設定の照合だけでは識別できません。
[ioctlの排他条件](https://github.com/torvalds/linux/blob/v7.0/drivers/media/cec/core/cec-api.c#L166-L195)

SIGKILLや電源断では解放できません。その場合は次回起動時に印を見つけて引き継ぎ、
その終了時に解放します。それまでの間は、他のアプリにキーが届く可能性があります。

### 状態表示

ワーカーを持たないため、状態は設定時の結果と、設定画面の表示時・再検索時に読み直した値で表します。

| 状態 | 意味 |
| --- | --- |
| `disabled` | 設定OFF、または起動ポリシーで無効 |
| `unsupported` | Linux以外 |
| `no-device` | `/dev/cec*` がない |
| `permission-denied` | 開く権限がない |
| `no-remote-control` | アダプターに `CEC_CAP_RC` がない |
| `configured` | ながめTVが設定した（物理アドレスがあれば表示する） |
| `external` | 外部の設定を使っている |
| `error` | 設定・解放の失敗 |

`waiting-for-tv`、`disconnected`、定期再試行を廃止しました。テレビの電源やケーブル状態への追従は
カーネルが行います。USBアダプターを後から接続した場合は、「再検索」で設定します。

## キー入力

カーネルの rc-cec キーマップと、xkb、Qtの変換表をたどると、各キーは次のように届きます。
xkbはPiの `xkeyboard-config` で、QtはqtbaseのソースでQt側の変換表を確認しました。
Qtでの受信は、まだ実機で確認していません。

| CECコード | カーネル | keysym | Qtのキー | ながめTVの動作 | 対応 |
| --- | --- | --- | --- | --- | --- |
| `0x01`〜`0x04` | `KEY_UP` 等 | `Up` 等 | `Key_Up` 等 | 方向移動 | 変更なし |
| `0x00` | `KEY_OK` | `XF86OK` | なし（`0`） | 決定 | 正規化で `Key_Return` にする |
| `0x2b` | `KEY_ENTER` | `Return` | `Key_Return` | 決定 | 変更なし |
| `0x0d` | `KEY_EXIT` | `XF86Close` | `Key_Close` | 戻る | 正規化で `Key_Back` にする |
| `0x71`〜`0x74` | `KEY_BLUE` 等 | `XF86Blue` 等 | `Key_Blue` 等 | データ放送のカラーキー | キーボード経路に処理を追加 |
| `0x30` / `0x31` | `KEY_CHANNELUP` / `DOWN` | `XF86ChannelUp` / `Down` | なし | 次 / 前の局 | 正規化で `Key_PageUp` / `Key_PageDown` にする |
| `0x20`〜`0x29` | `KEY_NUMERIC_0`〜`9` | `XF86Numeric0`〜`9` | なし | 番号選局 | 正規化で `Key_0`〜`9` にする（LGでは届かない） |
| `0x32` | `KEY_PREVIOUS` | なし | なし | 直前の局 | 対応しない |
| `0x44` / `0x60` | `KEY_PLAYCD` | `XF86AudioPlay` | `Key_MediaPlay` | 再生 | バインドを追加 |
| `0x46` | `KEY_PAUSECD` | `XF86AudioPause` | `Key_MediaPause` | 一時停止 | バインドを追加 |
| `0x61` | `KEY_PLAYPAUSE` | `XF86AudioPlay` | `Key_MediaPlay` | 再生 | 上と同じ |
| `0x45` / `0x64` | `KEY_STOPCD` | `XF86AudioStop` | `Key_MediaStop` | 停止 | 既存のバインド |
| `0x48` / `0x49` | `KEY_REWIND` / `KEY_FASTFORWARD` | `XF86AudioRewind` / `Forward` | `Key_AudioRewind` / `Key_AudioForward` | 10秒戻る / 進む | バインドを追加 |
| `0x4c` | `KEY_BACK` | `XF86Back` | `Key_Back` | **戻る**（従来は前のチャプター） | 区別できないため許容する |
| `0x4b` | `KEY_FORWARD` | `XF86Forward` | `Key_Forward` | 次のチャプター | バインドを追加 |

### 正規化フィルター

アプリ全体に `QKeyEvent` のイベントフィルターを1つ置きます。`nativeVirtualKey()` の keysym を見て、
Qtに対応するキーがないものや、意味を揃えたいものを既存のキーに変換します。QMLの `KeyEvent` は
`nativeVirtualKey` を公開しないため、変換はQtのネイティブ層で行います。CECはLinux専用なので、
xkbの keysym に依存して構いません。変換後は通常のキー入力と同じ経路をたどるため、
各パネルのD-pad処理や `ShortcutBindings` をそのまま使えます。

- **CEC以外のキー**: このフィルターは、同じ keysym を持つ普通のキーボードやリモコンにも効きます。
  たとえば `XF86Close` を持つキーボードでは、Closeキーが戻るとして働きます。テレビ向けの操作として妥当なので許容します。
- **入力デバイスは区別しない**: Qtのキーイベントには入力元のデバイス情報がありません。
  `0x4c`（前へスキップ）が `Key_Back` になるのはカーネルのキーマップによるもので、アプリ側では区別できません。

### 長押しとリピート

CEC専用の押下追跡（550 msの失効、早送り・巻き戻しだけのリピート許可、世代管理）は廃止します。
テレビがキーを押し続けて送る場合はカーネルの自動リピートになり、`QKeyEvent::isAutoRepeat()` で
キーボードと同じように扱います。

## `ACTIVE_SOURCE`（任意）

カーネルは `ACTIVE_SOURCE` を自分からは送りません。誰も initiator を排他にしていなければ、
`/dev/cecN` を開けるプロセスは誰でも送信できます。

- **送るとき**: [遠隔操作API](remote-control.md)で視聴を開始したときに1回だけ送り、テレビの入力をながめTVへ切り替えます。
- **前提**: 論理アドレスが有効なときに限ります。ながめTVの設定でも外部の設定でも構いません。
- **失敗時**: 送信の失敗は `tracing` で `WARN` として記録し、再試行しません。
- **応答しないもの**: `REQUEST_ACTIVE_SOURCE`、`ROUTING_CHANGE`、`SET_STREAM_PATH` には応答しません。

必須機能ではないため、移行の最後に別の変更として扱います。

## OS側で設定する構成

[Piキオスク](pi-appliance.md)のようにOSごと配布する構成では、起動時に次のoneshotユニットで
設定してもかまいません。

```sh
cec-ctl -d /dev/cec0 --playback --osd-name NagameTV
```

アプリから見るとこれは外部の設定なので、アプリのCEC設定がONでも解放しません。
`cec-ctl` はパススルーフラグを既定で立てます。

## 削除・変更する範囲

- **削除する**:
  - ワーカーと状態機械
  - `rust/src/hdmi_cec/protocol.rs`、`keys.rs`、`diagnostics.rs`
  - `rust/src/remote_key.h` のCEC用ヘルパー
  - `hdmi_cec_input_requested` シグナルと `Main.qml` の受信処理
  - `set_hdmi_cec_playback_visible` と `Main.qml` の呼び出し
  - 関連する試験
- **縮小する**: `rust/src/hdmi_cec/linux.rs` と `mod.rs`。デバイス列挙、能力確認、
  論理アドレスの読み書き、所有の印を残します。将来の送信は同じ非排他のデバイス経路に追加できます。
- **残す**:
  - 設定キー `hdmi_cec_enabled` と `hdmi_cec_device`、デバイス一覧のモデルと設定UI
  - `remote_key_requested`（遠隔操作APIが使う）
  - 実行時フラグ `hdmi_cec_enabled`（既定OFF）と `--features=hdmi-cec`
- **追加する**:
  - 正規化フィルター
  - カラーキー、メディアキー、`Key_Forward` のキーボード経路での処理
  - 所有の印
  - `no-remote-control` と `external` の状態
- **snap**: snapdの[`hdmi-cec`インターフェース](https://snapcraft.io/docs/reference/interfaces/hdmi-cec-interface/)は、
  2026-10-15にマージされました。2.78より後のため、2.79以降で使える見込みです。
  OSが提供する暗黙のスロットに手動で接続する方式で、snap-declarationは不要です。
  対応したsnapdが出たら、`custom-device` の plug と `cec-adapter` slot を `hdmi-cec` に置き換え、
  `assumes` でsnapdのバージョンを指定します。

## 移行手順

[トランクベース開発](development.md#トランクベース開発とフィーチャーフラグ)に従い、
各段階を独立して検証できる変更として取り込みます。

1. **キー入力の正規化**: フィルター、カラーキー・メディアキー・`Key_Forward` の処理を追加します。
   キーボードやメディアキーにも効く通常の入力処理です。
2. **CEC実装の置き換え（実装済み）**: 既定OFFの `hdmi_cec_enabled` の下で、ワーカー方式を一度の設定と
   所有の印に置き換えました。[開発ガイド](development.md)と[HDMI-CECの説明](hdmi-cec.md)に反映しました。
3. **Piキオスク**: 必要なら、`packaging/pi-appliance/` にoneshotユニットを追加します。
4. **`ACTIVE_SOURCE`**: 遠隔操作APIでの視聴開始時の送信を追加します。
5. **snap**: snapd 2.79以降の公開後に、`hdmi-cec` インターフェースへ切り替えます。

## 未確認事項

- **Qtでの受信**: labwc上のながめTVで、`nativeVirtualKey()` が `XF86OK` を返すこと、
  および `Key_Close` とカラーキーが届くことを確認する必要があります。
- **テレビ電源OFF時の実機試験**: ソースで確認した遅延取得の経路を、テレビ接続時の
  電源OFFからONの切り替えで確認します。
- **他のテレビ**: 確認したのはLG 1台だけです。電源状態への `FEATURE_ABORT` を受けて、キーの転送をやめるテレビがないか確認が必要です。
- **USBアダプター**: Pulse-Eightなどでパススルーが動くか。カーネルの既定能力には `CEC_CAP_RC` が
  含まれていますが、物理アドレスをユーザー空間から設定する必要があるアダプターは、この案でも引き続き対象外です。
- **デスクトップ環境のメディアキー**: GNOMEやKDEはメディアキーを横取りしてMPRISへ送ります。
  ウィンドウ内のバインドとの二重処理が起きないか、[デスクトップメディア連携](desktop-media.md)と合わせて確認します。
