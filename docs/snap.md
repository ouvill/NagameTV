# NagameTVのsnapパッケージ

Ubuntu 26.04 DesktopのWaylandセッションと、Ubuntu Core上のUbuntu Frameを対象に、
strict confinementのsnapをamd64・arm64向けに作ります。組み込み環境ではRaspberry Pi 4を
最初の検証対象とします。各環境の実機検証項目は末尾に記載します。

## ベースと依存ライブラリー

NagameTVはQt 6.8以降（Qt Quick、Qt Wayland、Qt WebEngineを含む）と
GStreamer 1.24以降（`qml6glsink`、MPEG-TS、デコーダー、音声出力を含む）を使います。
Ubuntu 24.04のGStreamer 1.24は要件を満たします。標準Qtは6.4.2のため、
24.04用AppImageとdebは公式Qt SDKの6.8.3を同梱しています。そのSDKには
必要なarm64版WebEngineがありません。Ubuntu 26.04のQt 6.10.2とGStreamer 1.28.2は
両アーキテクチャーで要件を満たすため、snapのベースを`core26`にします。
[AppImageの構成](appimage.md)、[debの構成](deb.md)、
[UbuntuのQt 6パッケージ](https://packages.ubuntu.com/resolute/qt6-base-dev)、
[GStreamer Qt 6プラグイン](https://packages.ubuntu.com/resolute/gstreamer1.0-qt6)

QtとGStreamerは同じUbuntuアーカイブから`stage-packages`で同梱します。
`qml6glsink`が参照するQtと、本体・QMLモジュールが参照するQtの組み合わせをそろえられます。
KDEの`kde-neon-6`／`kde-neon-qt6`はQtの共有ランタイムを提供する選択肢です。
採用する場合は、ランタイム内のQtに合わせたGStreamerプラグインとWebEngineを検証します。
GNOME拡張はGNOMEの共有ランタイムを提供するため、QtとWebEngineの供給方法は別途必要です。
[Snapcraftの拡張一覧](https://ubuntu.com/docs/snapcraft/9/reference/extensions/)

GPUのユーザー空間ドライバーは`mesa-2604`から`gpu-2604` content interfaceで取得します。
`gpu-2604`は`graphics-core22`の後継です。NagameTVはWaylandとEGLを使い、Ubuntu Frameは
ディスプレイと入力を管理します。FrameとNagameTVはそれぞれのベースに対応するGPUプロバイダーを
使い、Waylandソケットで通信します。Frameの`24/stable`を初回検証に使います。
[Ubuntu FrameのGPUインターフェース](https://ubuntu.com/frame/docs/26/how-to/use-snap-graphics/)、
[Ubuntu Coreのテスト用イメージ](https://documentation.ubuntu.com/core/reference/testing-platforms/)

## 権限

| インターフェース | 用途と接続条件 |
| --- | --- |
| `wayland`、`opengl`、`gpu-2604` | 表示とGPUアクセスに必要。DesktopはセッションのWaylandを使い、Coreでは`ubuntu-frame:wayland`に接続する。 |
| `desktop`、`screen-inhibit-control` | Desktopのファイル選択・文書ポータルとスクリーンセーバー抑止に使用。Coreではセッションサービスの提供状況に応じてQt Quickのダイアログを使う。logindの`Inhibit`呼び出しは要検証。`login-session-observe`は情報参照用で、`login-session-control`は今回の用途より広いセッション管理権限を持つ。 |
| `audio-playback` | PulseAudio互換サービスへの再生用。DesktopはPipeWireのPulseAudio互換ソケットを使う。CoreでPulseAudio互換サービスを使う場合は、対応スロットとシステムサービスから到達できるソケットが必要。 |
| `alsa` | Coreで音声サーバーを用意せず、ALSAデバイスを直接使うときに手動接続する。HDMI出力の選択は実機で確認する。 |
| `network`、`network-bind` | 配信サーバー・関連サービスへの接続と、リモート操作やデータ放送で使用するローカルサーバーの待ち受け。リモート操作はアプリの設定で有効にする。 |
| `home`、`removable-media` | 録画ファイルの読み込みとスクリーンショットの保存に使用。`home`からはホーム直下の隠し領域への一般アクセスは得られない。メディアを使う場合は`removable-media`を手動接続する。 |
| `hdmi-cec` | `/dev/cec*`を使う場合に手動接続し、アプリの設定でHDMI-CECを有効にする。デバイスノード自体の権限も必要。 |
| `camera`、`media-control` | V4L2の`/dev/video*`とstateless decoderで使う`/dev/media*`への任意アクセス。接続にはカメラやメディアデバイスへのアクセスも伴うため、必要な実機に限って手動接続する。通常の再生には同梱のlibavを使える。 |

[Snapインターフェース一覧](https://snapcraft.io/docs/reference/interfaces/)、
[`camera`のポリシー](https://github.com/canonical/snapd/blob/master/interfaces/builtin/camera.go)、
[`media-control`のポリシー](https://github.com/canonical/snapd/blob/master/interfaces/builtin/media_control.go)

## パッケージを作る

Snapcraft 9とUbuntu 26.04のLXDビルド環境、または使い捨てのUbuntu 26.04環境が必要です。
ビルドはネイティブで行い、amd64とarm64はそれぞれ対応するマシンを使います。
Gitのサブモジュールを初期化してから、リポジトリーのルートで実行します。

```sh
git submodule update --init --recursive
python3 -m scripts.packaging.build_snap
```

CIはUbuntu 26.04の使い捨てランナー上で`--destructive-mode`を使い、
`snap-amd64`と`snap-arm64`をActionsの成果物に保存します。ローカルとCIの出力先は
`build/snap/`で、snapとSHA-256を生成します。試験用の成果物はGitHub Releaseや
Snap Storeへの自動公開の対象外です。
[Snapcraftのビルド環境](https://ubuntu.com/docs/snapcraft/9/reference/build-environment-options/)

## Ubuntu Desktopに導入する

実機と同じアーキテクチャーの成果物を展開し、同じディレクトリーで実行します。
`--dangerous`はローカル成果物のストア署名を省略するために使います。
パッケージはstrict confinementで動作します。

```sh
sha256sum -c nagametv_*.snap.sha256
sudo snap install mesa-2604
sudo snap install --dangerous ./nagametv_*.snap
snap connections nagametv
```

`gpu-2604`が未接続の場合は`sudo snap connect nagametv:gpu-2604 mesa-2604:gpu-2604`を
実行します。`wayland`が未接続の場合は`sudo snap connect nagametv:wayland`を実行します。
ログイン中のWaylandセッションの端末で`snap run nagametv`を実行し、設定画面で
Mirakurun／mirakcと必要に応じてEPGStationを設定します。`snap run nagametv --kiosk`で
同じユーザーデータを使って全画面のキオスク表示を試せます。

Desktopの設定ファイルは`$SNAP_USER_DATA/.config/nagametv/settings.toml`
（通常`~/snap/nagametv/current/.config/nagametv/settings.toml`）に保存されます。
キャッシュ・データ・状態も同じリビジョンのXDGディレクトリーを使います。
既存の録画を開く場合や画像を通常の「ピクチャ」へ出す場合は、アクセス可能なフォルダーを選択します。
USBメディアを使う場合は`sudo snap connect nagametv:removable-media`を実行します。

## Ubuntu Coreに導入する

Raspberry Pi 4ではarm64版、PCではamd64版を用意します。Ubuntu Coreの
テスト用イメージで機器を起動し、SSHから次を実行します。ローカルsnapを
導入できるモデルが必要です。既存のDesktop環境で試す場合は前節を使います。

```sh
sudo snap install ubuntu-frame --channel=24/stable
sudo snap set ubuntu-frame daemon=true
sudo snap install mesa-2604
sudo snap install --dangerous ./nagametv_*.snap
sudo snap connect nagametv:wayland ubuntu-frame:wayland
snap connections nagametv
```

`gpu-2604`が未接続の場合はDesktopと同じコマンドで接続します。音声サーバーのない機器で
直接ALSAを使う場合は`sudo snap connect nagametv:alsa`を実行します。
PulseAudio互換サービスがある場合は、そのサービスの`audio-playback`スロットに接続します。
音声出力の準備後にサービスを有効にします。サービスはUbuntu Frameのソケットを待ってから
`--kiosk`で起動し、異常終了時はsystemdが再起動します。

```sh
sudo snap start --enable nagametv.kiosk
snap services nagametv
sudo snap logs -n=80 nagametv.kiosk
```

サービスはrootで動き、設定を`$SNAP_DATA/.config/nagametv/settings.toml`
（通常`/var/snap/nagametv/current/.config/nagametv/settings.toml`）に保存します。
リビジョンごとのキャッシュ・データ・状態と、初期保存先の`Pictures`も同じ配下にあります。
初回は画面の設定から接続先・選局・必要に応じて自動再生を保存します。
設定から終了したあとに画面へ戻す場合は`sudo snap restart nagametv.kiosk`を実行します。
機器の起動時にサービスを起動する設定を解除する場合は
`sudo snap stop --disable nagametv.kiosk`を実行します。
[Waylandクライアントの起動方法](https://ubuntu.com/frame/docs/26/explanation/iot-gui-snaps-made-easy/)、
[snapのデータディレクトリー](https://snapcraft.io/docs/reference/development/environment-variables/)

## 実機検証項目

- amd64 DesktopとPi 4 Desktopで、strictのままWayland表示、HDMIまたは既定出力への音声、
  ライブと録画の再生、再起動後の設定復元、スクリーンショット保存を確認します。
- Ubuntu Coreで、Frameとサービスの起動順、再起動後の自動起動、ALSAまたはPulseAudio互換
  サービスでのHDMI音声、CEC、ログの権限エラーを確認します。Frameの`26/stable`は
  2026年11月18日時点で未公開です。
- Pi 4の既存Desktop環境では、1080iをGreedyLで処理したときCPU使用率が約254%でした。
  ハードウェアH.265デコーダーが出力するBroadcom SANDを表示に使えず、`avdec_h265`に
  切り替わりました。snap版での挙動と1080p60表示での余裕は実機で確認します。
- データ放送は既定で無効です。Qt WebEngineのChromiumサンドボックスは、strict confinementの
  デスクトップ起動とrootのキオスク起動で検証が必要です。サンドボックスを無効にする
  環境変数は設定していません。
  [Qt WebEngineのプラットフォーム要件](https://doc.qt.io/qt-6/qtwebengine-platform-notes.html)
