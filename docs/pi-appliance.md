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
| `provision-nagametv.sh`、`nagametv-kiosk.service`、`start-kiosk`、`stop-kiosk.py`、`labwc-rc.xml` | 配布用ファイルをそのままコピーする。1つでも欠けると初回設定は何も導入せずに止まる |
| `nagametv-hdmi-audio.service`、`hdmi-audio.py`、`50-nagametv-hdmi-only.conf`、`50-nagametv-hdmi-stream.conf` | HDMI0の音声設定と復旧用。4ファイルともそのままコピーする |
| `nagametv.deb` | 照合済みのUbuntu 26.04 arm64用debをこの名前でコピーする |
| `nagametv-connection.toml`（任意） | [接続先の例](../packaging/pi-appliance/nagametv-connection.toml.example)をこの名前でコピーし、URLを編集する |
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

### 接続先をカードに入れる

初回起動から視聴する場合は、`system-boot` 直下の `nagametv-connection.toml` に
MirakurunまたはmirakcのURLを書きます。EPGStationを使う場合は2行目も記入します。

```toml
server = "http://192.168.1.100:40772"
epgstation_server = "http://192.168.1.100:8888"
```

URLは `http://` または `https://` で始めます。ユーザー名、パスワード、クエリー、
フラグメントを含めず、値は引用符で囲みます。初回設定はながめTVの取り込みコマンドを
`nagametv` ユーザーとして実行し、`~nagametv/.config/nagametv/settings.toml` に
接続先と自動再生を保存します。チャンネル一覧を取得できると、初回は一覧の最初の局を再生します。
取り込みコマンドに対応したdebを使ってください。ファイルの内容が不正な場合は、
エラーをコンソールとjournalに出し、キオスクを起動する前に初回設定を停止します。

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
PipeWire、PipeWireのPulseAudio互換ソケット、WirePlumberを使います。kioskユーザーは `audio` グループに属します。
WirePlumberではPi 4のアナログ出力を無効にし、HDMI0の出力に `nagametv-hdmi0` という名前を付けます。
起動時はHDMI音声を数秒待ち、応答がなければjournalに理由を記録して画面を起動します。
視聴時の音声はHDMI0に接続し、出力が消えている間は復帰を待ちます。音の大きさはテレビとアプリで調節します。

ながめTVが終了するとlabwcも終了し、2秒後に起動し直します。60秒以内に5回起動しても安定しない場合は、
systemdが以降の起動を止めます。原因を取り除いたら `sudo systemctl reset-failed nagametv-kiosk.service` を
実行し、サービスを起動してください。保守中は `sudo systemctl stop nagametv-kiosk.service` で停止できます。

`systemctl stop` と `systemctl restart` では、`ExecStop` がながめTVに `SIGTERM` を送り、
プロセスの終了を待ちます。labwcはセッションクライアントの終了を受けて終了します。これにより、
ながめTVはWayland接続がある間にCECの解放と設定保存を行えます。`ExecStop` の待機が20秒に
達すると、systemdは残っているプロセスの停止に進みます。
[systemdの停止処理](https://www.freedesktop.org/software/systemd/man/latest/systemd.service.html#ExecStop=)
[labwcのセッションクライアント監視](https://github.com/labwc/labwc/blob/0.9.3/src/server.c#L146-L182)

停止コマンドの `+` は、tty1で2つ目のPAMセッションを開こうとして停止が失敗するのを防ぎます。
停止ヘルパーはrootで起動し、キオスクの実行時ディレクトリを開く前に `nagametv` ユーザーへ
権限を落とします。ビューアーの親プロセスと、ビューアー・labwcの起動時刻が一致した場合に
終了シグナルを送ります。
[systemdのコマンド接頭辞](https://www.freedesktop.org/software/systemd/man/latest/systemd.service.html#Command%20Lines)

labwcを先に終了させると、QtはWayland接続の切断時にアプリを即時終了することがあります。
CECの設定と印が残った場合は、次の起動が引き継ぎます。
[Qt 6.10のWaylandエラー処理](https://github.com/qt/qtbase/blob/v6.10.2/src/plugins/platforms/wayland/qwaylanddisplay.cpp#L505-L517)


専用機のunitでは `NAGAMETV_DEINTERLACE=greedyl` と `NAGAMETV_DEINTERLACE_FIELDS=one` を
指定します。Pi 4で地デジの1080iを視聴したとき、GreedyLの両フィールド処理は描画46.5枚/s、
破棄0.40枚/s、CPU使用率240%でした。専用機はGreedyLの動き適応処理を使い、出力を29.97 fpsに
抑えます。unitの指定は起動時に保存済みの方式よりも優先されます。[計測条件と比較](gpu-video.md)

初回は次を確認します。

1. テレビの信号情報が1920×1080の60Hzまたは59.94Hzを示し、ながめTVの設定画面または視聴画面が出る。
2. 接続先ファイルを入れた場合は、自動再生が始まることを確かめる。ファイルを入れなかった場合は、
   初回設定がローカルネットワークを検索する。上下でMirakurunまたはmirakcを選び、決定で接続する。
   見つからないときは再検索するか、手動でURLを入力する。「設定 → 接続」ではEPGStationも検索できる。
   視聴中はHDMI音声が出ることを確かめる。HDMI-CECを使う場合は
   [設定ガイド](hdmi-cec.md)に沿って有効にする。kioskユーザーは `video` グループに属し、
   アプリのCEC設定は既定のOFFから始まる。初回に方向キーの入力を用意できない場合は、
   カードに接続先ファイルを入れて起動する。
3. 視聴を続け、入力に触れずにテレビの普段の消灯時間を超えても映像と音声が続くことを確かめる。
   Linuxのコンソール消灯は `consoleblank=0` で無効にし、labwcのセッションにはアイドル時に
   消灯するデスクトップサービスを起動しない。テレビ側の無操作電源オフはテレビの設定で扱う。
4. 初回セットアップと区別して、次の起動では電源投入から最初のアプリ画面までを計る。
   自動再生を設定した場合は最初の映像までの時間も記録する。
   `sudo journalctl -b -u nagametv-kiosk.service -u nagametv-hdmi-audio.service -o short-monotonic` で
   アプリの起動と音声の準備完了時刻を確認できる。

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

既存の専用機にシグナルによる終了と停止順序の修正を適用するときは、修正済みのdebを導入します。
更新後の `packaging/pi-appliance/` をPiに転送し、そのディレクトリーで次を実行します。

```bash
sudo install -D -m 0755 start-kiosk /usr/local/lib/nagametv-appliance/start-kiosk
sudo install -D -m 0644 stop-kiosk.py /usr/local/lib/nagametv-appliance/stop-kiosk.py
sudo install -m 0644 nagametv-kiosk.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl restart nagametv-kiosk.service
```

停止コマンドのPAM修正を既存の専用機へ適用する場合は、サービスを停止してから
上の3ファイルを一緒に更新します。起動スクリプトが記録するPIDの形式も変わるためです。
この修正だけならdebの再導入は不要です。新しいunitで起動した後、次の停止でCECが
解放されることを確認します。


ルートファイルシステムは標準どおり書き込み可能です。将来、読み取り専用ルートに移行する際は、
永続データ領域を先にマウントし、systemd unitのdrop-inから
[`NAGAMETV_DATA_DIR`](development.md#保存データを別のパーティションに置く)をそのパスに設定できます。

## HDMIをつなぎ直しても音が出ないとき

HDMIケーブルを抜き差ししたりテレビの入力を切り替えたりした直後に、`vc4-hdmi-0` の
プロファイルが `output:hdmi-stereo` のままでも、音声の出力先だけが消えることがあります。
そのときは `wpctl status` に `Dummy Output` が現れたり、journalに `playback open failed: Unknown error 524` が
記録されたりします。

`nagametv-hdmi-audio.service` はkioskと一緒に動きます。`HDMI-A-1` が接続・有効になり、テレビの
EDIDを読める状態でHDMI0の出力が見つからなければ、kioskユーザーのWirePlumberを最大3回再起動します。
出力が戻ると既定の出力に設定し、音量を100%にそろえます。再試行を使い切った場合は、
テレビを点けてPiの入力に切り替え、HDMI0につなぎ直してください。接続が一度失われて戻ると再試行が始まります。
出力がない間に視聴を開始して再生エラーが出た場合は、出力が戻ってからチャンネルを選び直してください。

PiにSSHで接続し、kioskユーザーの音声状態とログを確認します。`Sinks` に `nagametv-hdmi0` があるか、
視聴中の `Streams` がその出力につながっているかを確認してください。

```bash
kiosk_uid=$(id -u nagametv)
sudo -u nagametv env XDG_RUNTIME_DIR="/run/user/$kiosk_uid" wpctl status -n
sudo journalctl -b -u nagametv-kiosk.service -u nagametv-hdmi-audio.service
sudo journalctl -b _UID="$kiosk_uid" _SYSTEMD_USER_UNIT=wireplumber.service _SYSTEMD_USER_UNIT=pipewire.service
```

出力が戻らない場合はログを保存し、`sudo systemctl restart nagametv-hdmi-audio.service` で再試行できます。

既存の専用機にHDMIの復旧設定を追加する場合は、更新後の `packaging/pi-appliance/` をPiに転送します。
転送したディレクトリーで次を実行し、再起動して音声サービスに設定を読み込ませます。

```bash
sudo install -D -m 0755 start-kiosk /usr/local/lib/nagametv-appliance/start-kiosk
sudo install -D -m 0644 stop-kiosk.py /usr/local/lib/nagametv-appliance/stop-kiosk.py
sudo install -D -m 0644 hdmi-audio.py /usr/local/lib/nagametv-appliance/hdmi-audio.py
sudo install -m 0644 nagametv-kiosk.service nagametv-hdmi-audio.service /etc/systemd/system/
sudo install -D -m 0644 50-nagametv-hdmi-only.conf /etc/wireplumber/wireplumber.conf.d/50-nagametv-hdmi-only.conf
sudo install -D -m 0644 50-nagametv-hdmi-stream.conf /etc/pipewire/pipewire-pulse.conf.d/50-nagametv-hdmi-stream.conf
sudo reboot
```
