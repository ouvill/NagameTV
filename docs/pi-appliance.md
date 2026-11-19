# Raspberry PiでながめTV専用機を作る

Raspberry Pi 4または5にUbuntu Server 26.04 LTSのarm64版を入れます。初回起動時に
起動パーティションの設定をcloud-initが読み、Ubuntuのパッケージと手元のながめTVのdebを導入します。
以降は電源を入れると、HDMIの画面にlabwcのWaylandセッションとながめTVが起動します。
Ubuntu 26.04のCage 0.2.1は、表示前に全画面を要求するウィンドウで異常終了するため使いません
（[cage-kiosk/cage#456](https://github.com/cage-kiosk/cage/issues/456)）。

用意するものは、Pi用の起動メディア、HDMI0につないだテレビとその音声出力、初回起動時に
Ubuntuのアーカイブへ到達できる有線LANまたはWi-Fi、管理用SSH公開鍵です。
ながめTVのdebは起動メディアから読み込みます。labwcやQtなどの依存パッケージは、
初回起動時にUbuntuのアーカイブから取得します。

## イメージとdebを用意する

1. [UbuntuのRaspberry Piインストールガイド](https://ubuntu.com/hardware/docs/boards/how-to/ubuntu_supported/raspberry-pi/)から
   `ubuntu-26.04.1-preinstalled-server-arm64+raspi.img.xz`（または26.04 LTSの同等の更新版）を選び、
   [配布ディレクトリー](https://cdimage.ubuntu.com/releases/26.04/release/)のSHA-256と照合します。
2. GitHubにログインし、対象コミットの
   [Actions実行](https://github.com/ouvill/NagameTV/actions/workflows/ci.yml)から
   `deb-ubuntu26.04-arm64`をダウンロードして展開します。
   CLIでは `gh run download RUN_ID --repo ouvill/NagameTV --name deb-ubuntu26.04-arm64` を使えます。
   CIの成果物は同梱の `.deb.sha256` を `sha256sum -c` で検査します。
   [GitHub Releases](https://github.com/ouvill/NagameTV/releases)に
   `nagametv_*ubuntu26.04_arm64.deb`がある場合は、添付ファイルに表示されるハッシュと
   `sha256sum` の出力を照合して使用できます。

## 書き込んでsystem-bootを編集する

Raspberry Pi ImagerではOSの「カスタムイメージを使う」から取得したイメージを選びます。
対象のカードを指定し、OSカスタマイズは省略して書き込みます。完了後に再挿入し、
最初のFATパーティション `system-boot` をマウントします。

Linuxで `dd` を使う場合は、`lsblk -o NAME,SIZE,MODEL,MOUNTPOINTS` でカードを特定し、
カード上のパーティションをアンマウントします。`of=` にはカード全体のデバイスを指定します。
選んだデバイスの内容は消去されます。

```bash
xz -dk ubuntu-26.04.1-preinstalled-server-arm64+raspi.img.xz
sudo dd if=ubuntu-26.04.1-preinstalled-server-arm64+raspi.img of=/dev/sdX bs=4M conv=fsync status=progress
sync
```

書き込み後にカードを再挿入して `system-boot` をマウントします。
[配布用ファイル](../packaging/pi-appliance/)を使い、FATパーティションの直下を次のように編集します。

| 名前 | 操作 |
| --- | --- |
| `user-data` | 同名ファイルで上書きし、`REPLACE_WITH_YOUR_SSH_PUBLIC_KEY` を公開鍵1行で置き換える |
| `network-config` | 有線は同名ファイルで上書き。Wi-Fiは `network-config.wifi` をこの名前に変えてコピーし、SSIDとパスワードを編集する |
| `provision-nagametv.sh`、`nagametv-kiosk.service`、`start-kiosk`、`labwc-rc.xml` | 配布用ファイルをそのままコピーする。1つでも欠けると初回設定は何も導入せずに止まる |
| `nagametv.deb` | 照合済みのUbuntu 26.04 arm64用debをこの名前でコピーする |
| `current/cmdline.txt` | 既存の1行を残し、末尾に半角空白と `video=HDMI-A-1:1920x1080@60D consoleblank=0` を追加する |

Wi-Fiの例は日本向けの `regulatory-domain: JP` を含みます。SSIDとパスワードはYAMLの
文字列として記述します。二重引用符やバックスラッシュが値に含まれる場合はエスケープしてください。
パスワードはFAT上に平文で保存されるため、カードは管理者の手元で扱います。SSHには公開鍵を入れます。

Ubuntu 26.04のPi用イメージはA/B起動を使い、カーネルなどの起動ファイルを `current/` に置きます。
カーネル更新時の `new/cmdline.txt` は `current/cmdline.txt` から作られるため、追加した指定は更新後も残ります。
[A/B起動の説明](https://ubuntu.com/hardware/docs/boards/explanations/piboot-ab/)

`cmdline.txt` の `video=` は起動時のKMS出力を1080pの60Hzに指定します。HDMI0は
Linuxでは `HDMI-A-1` です。既存の `root=` やコンソール指定は維持します。
Pi 4で4K60を選ぶ場合にはファームウェア設定 `hdmi_enable_4kp60=1` が関係します。
この構成は4Kテレビでも1080pを使うため、その設定は不要です。
labwcの起動時にはEDIDの優先モードが選ばれる場合があるので、アプリ起動前に `wlr-randr` でも1920×1080の60Hzを選びます。
60.000Hzが選べない場合は、テレビが59.940Hzを提示していればそちらを使います。
[Piの出力モード指定](https://www.raspberrypi.com/documentation/computers/configuration.html#set-the-kms-display-mode)

## 初回起動と動作確認

カードを安全に取り外し、Piにセットします。テレビを点けてHDMI0を接続し、
有線の場合はLANケーブルをつないで電源を入れます。最初の起動にはaptのダウンロードと
インストールの時間が加わります。初回設定の各段階は `NagameTV appliance:` で始まる行として
テレビのコンソールにも表示されます。Ubuntuの自動更新が先に始まっていた場合は、その完了を待ってから
パッケージを導入します。起動中の状況は `ssh tvadmin@PiのIPアドレス` で接続して確認できます。

```bash
cloud-init status --wait
sudo journalctl -b -u cloud-final.service -u nagametv-kiosk.service
```

管理アカウントは `tvadmin` です。公開鍵で接続し、`sudo` で保守できます。labwcとながめTVは
管理権限のない `nagametv` ユーザーで動きます。systemdがtty1を割り当て、
PAMとlogindがユーザーセッションと `XDG_RUNTIME_DIR` を作ります。音声は同じユーザーの
PipeWire、PipeWire PulseAudio互換ソケット、WirePlumberを使います。kioskユーザーは `audio` グループに属し、
WirePlumberではPi 4のアナログ出力を無効にします。起動スクリプトは既定の出力がHDMIでなければ停止し、
HDMI出力の音量を100%にそろえます。音の大きさはテレビとアプリで調節します。ながめTVが終了するとlabwcも終了し、
2秒後に起動し直します。保守中は `sudo systemctl stop nagametv-kiosk.service` で停止できます。

専用機のunitでは `NAGAMETV_DEINTERLACE=greedyl` と `NAGAMETV_DEINTERLACE_FIELDS=one` を
指定します。Pi 4で地デジの1080iを視聴したとき、GreedyLの両フィールド処理は描画46.5枚/s、
破棄0.40枚/s、CPU使用率240%でした。専用機はGreedyLの動き適応処理を使い、出力を29.97 fpsに
抑えます。unitの指定は起動時に保存済みの方式よりも優先されます。[計測条件と比較](gpu-video.md)

初回は次を確認します。

1. テレビの信号情報が1920×1080の60Hzまたは59.94Hzを示し、ながめTVの設定画面または視聴画面が出る。
2. アプリの初回設定でチューナーを登録して視聴し、HDMI音声が出る。HDMI-CECを使う場合は
   [設定ガイド](hdmi-cec.md)に沿って有効にする。kioskユーザーは `video` グループに属し、
   アプリのCEC設定は既定のOFFから始まる。
3. 視聴を続け、入力に触れずにテレビの普段の消灯時間を超えても映像と音声が続くことを確かめる。
   Linuxのコンソール消灯は `consoleblank=0` で無効にし、labwcのセッションにはアイドル時に
   消灯するデスクトップサービスを起動しない。テレビ側の無操作電源オフはテレビの設定で扱う。
4. 初回セットアップと区別して、次の起動では電源投入から最初のアプリ画面までを計る。
   自動再生を設定した場合は最初の映像までの時間も記録する。
   `sudo journalctl -b -u nagametv-kiosk.service -o short-monotonic` で
   モードと音声の準備完了時刻を確認できる。

## 起動時のサービスと保守

| 項目 | 方針 |
| --- | --- |
| aptの自動更新 | `apt-daily.timer` と `apt-daily-upgrade.timer` をマスクする。起動と同時の取得やインストールを避け、管理者が保守時に `sudo apt update && sudo apt upgrade` を実行する |
| snapd | snapを使わないため、デーモン・ソケット・初期化・自動インポートをマスクする |
| cloud-init | 初回のパッケージ導入とunit登録が成功した時点で `/etc/cloud/cloud-init.disabled` を作る。2回目以降は既存のユーザーと生成済みのNetplan設定を使う |
| fwupdとModemManager | 自動のファームウェア照会とモバイル回線の管理を止める。ファームウェア更新は保守時に扱う |
| Plymouthとtty1 | `plymouth-quit-wait.service` をマスクし、Plymouthを終了するサービスは維持する。tty1はキオスク専用にし、他のVTとSSHは保守に使える |
| AppArmor | セキュリティ境界として維持する |

別構成のPiで測ったUbuntu Desktopの起動は2分27秒で、`systemd-analyze blame` に
apt-daily 47秒、Plymouthの待機46秒、cloud-init-local 22秒、AppArmor 18秒が記録されました。
各unitは並列に動くため、これらの値の合計は短縮時間を表しません。
このレシピの起動時間と消灯の有無は新しいカードの実機確認で記録します。

初回に失敗した場合は `sudo journalctl -b -u cloud-final.service` を確認し、
ネットワークまたはdebを直してから `sudo bash /boot/firmware/provision-nagametv.sh` を実行します。
labwcの起動やHDMI出力の失敗は `sudo journalctl -b -u nagametv-kiosk.service` で確認します。
debの更新は26.04 arm64用をSSHで転送し、
`sudo apt install ./nagametv.deb && sudo systemctl restart nagametv-kiosk.service` で行います。

ルートファイルシステムは標準どおり書き込み可能です。将来、読み取り専用ルートに移行する際は、
永続データ領域を先にマウントし、systemd unitのdrop-inから
[`NAGAMETV_DATA_DIR`](development.md#保存データを別のパーティションに置く)をそのパスに設定できます。
