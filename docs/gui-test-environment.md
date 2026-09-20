# GUIテストの専用環境

LinuxのGUIテストは、テスト専用の画面・入力・D-Busセッションと仮想音声出力を
作成します。Workshop内でも、FedoraなどのLinux上でも同じスクリプトを使います。
画面はデスクトップから分離し、音声は起動済みのPipeWireへ接続します。
WorkshopではホストからGPUだけを共有し、PipeWireはコンテナー内で起動します。

| 資源 | 使用する環境 |
|---|---|
| 描画 | ホストから共有した実GPU。ソフトウェア描画は拒否します。 |
| 画面・入力 | WestonのheadlessバックエンドとGLレンダラー、専用XwaylandとOpenbox。1920×1080の仮想画面です。 |
| 音声 | 起動済みのPipeWire／pipewire-pulse上に実行ごとの`module-null-sink`とmonitorを作成。スピーカーへ音を出しません。 |
| セッションバス | 専用`dbus-daemon`。ホストのPortalには接続しません。 |

これは明示的に選択したテスト環境です。GPUが利用できない場合の代替処理ではありません。
物理モニター、GNOME/KDE固有のウィンドウ操作、実音声デバイスの遅延や聴取品質は
この環境では確認できません。GPUの処理能力はホストの作業と共有します。

Xwaylandはrootfulモードで起動し、X11ウィンドウの管理をOpenboxに任せます。
物理入力機器のないWestonのrootless構成では、ダイアログを閉じた後に親画面へ
フォーカスが戻らない場合があります。専用のX11ウィンドウマネージャーにより、
キーボード操作やダイアログの開閉をテスト内で完結させます。

`NAGAMETV_TEST_QPA=wayland`または`auto`で映像処理・キャプチャの試験をWayland上で実行できます。
この環境のkiosk-shellはWaylandウィンドウを全画面に固定するため、任意のウィンドウサイズや
フォーカス・キーボード操作を含む起動試験全体はX11で実行します。

## Workshopの準備

[Workshop定義](../.workshop/dev.yaml)のGUI用接続は`gui:gpu → system:gpu`のみです。
Weston、Xwayland、PipeWire、WirePlumberなどは[プロジェクトSDK](../.workshop/nagametv/hooks/setup-base)で
導入します。既存Workshopへの定義変更の反映方法は[開発手順](development.md)を参照してください。
PulseAudioのサーバーパッケージは外し、`pactl`／`parec`を含む`pulseaudio-utils`は残します。
[GUI SDKのsetup-project](../.workshop/gui/hooks/setup-project)がPipeWire、pipewire-pulse、
WirePlumberをユーザーサービスとして起動します。テストはこれらを起動・停止しません。
WirePlumberの音声・MIDI・Bluetooth・カメラの自動検出はWorkshop内だけで無効にし、
仮想出力への接続管理を使います。旧SDKの`PULSE_SERVER=tcp:127.0.0.1:4713`も除去します。
GPU接続後の`setup-project`で`ldconfig`も実行し、マウントされたNVIDIA EGLライブラリーの
SONAMEリンクとキャッシュを更新します。

変更を反映するにはホスト側で`workshop refresh`を実行してください。
コンテナー内の手動インストールを前提にしません。

ホスト側で環境の検証だけを実行する場合:

```sh
workshop run -- gui-test --check
```

Workshop内では次のコマンドを使います。

```sh
python3 scripts/run-gui-tests.py --check
```

## FedoraなどのLinuxで直接実行

Workshopは不要です。[ネイティブのビルド依存](development.md#ネイティブ版をビルドする)に加え、
Weston、Xwayland、Openbox、`xdpyinfo`、`xprop`、`glxinfo`、D-Bus、Python 3.11以降、
GStreamerの`gst-launch-1.0`／`pulsesink`／`audiotestsrc`、`pactl`、`parec`が必要です。
通常のユーザーで実行し、実GPUへアクセスできる状態にしてください。
音声サーバーはPipeWireとPulseAudio互換サーバー、接続管理はWirePlumberを使います。
Fedoraの互換サーバーパッケージ名は`pipewire-pulseaudio`です。
PulseAudio本体への置き換えやWorkshop用SDKフックの実行は不要です。

既存サービスを確認してから、リポジトリーのルートで実行します。

```sh
systemctl --user is-active pipewire.service pipewire-pulse.service wireplumber.service
python3 scripts/run-gui-tests.py --check
bash scripts/test-startup.sh
```

接続先は、指定済みの`PULSE_SERVER`、未指定なら起動元の
`$XDG_RUNTIME_DIR/pulse/native`（変数未設定時は`/run/user/UID/pulse/native`）です。
`PULSE_SERVER`の指定は単一の絶対パス`unix:/...`に限り、TCPや接続先リストは拒否します。
テスト用に`XDG_RUNTIME_DIR`を分離する前に接続先を確定します。
ソケットがない場合やPipeWire以外の場合は停止し、別のサーバーへ切り替えません。

実行ごとにランダムな固有名の仮想sinkを作り、テストの再生先と録音先を
そのsink／monitorへ固定します。既定出力や他の出力・音量・ストリームを変更する
コマンドは実行しません。仮想sinkの優先度は低くし、テストストリームには
別出力へのフォールバック・移動・再接続を禁止するプロパティを付けます。
デスクトップと音声サーバーの処理負荷は共有されます。

## テストの実行

通常のGUIテストスクリプトは専用画面・D-Busを自動起動し、起動済みのPipeWireに
仮想出力を作成します。表示先の環境変数を手動で設定する必要はありません。

```sh
bash scripts/test-screenshot.sh
bash scripts/test-startup.sh
bash scripts/test-pointer-activity.sh
NAGAMETV_TEST_QPA=wayland bash scripts/test-startup.sh screenshot-playback
NAGAMETV_TEST_QPA=auto bash scripts/test-startup.sh video-processing
```

Workshopを使う場合はホストから次のように実行できます。

```sh
workshop exec -- bash -lc 'bash scripts/test-startup.sh'
```

複数の試験を一つの専用セッションで実行することもできます。各試験の設定用一時ディレクトリーは
従来どおり個別に作成されます。

```sh
python3 scripts/run-gui-tests.py -- bash -euc '
  bash scripts/test-screenshot.sh
  bash scripts/test-startup.sh
'
```

字幕試験の`--ui-only`は従来の明示的なソフトウェア描画許可モードです。このモード自体は
専用画面を作成しません。ホストのデスクトップへ接続せず、別途用意して検証した
テスト専用X11環境で使用します。通常は引数なしのGPU試験を使ってください。

機器不要のRust・接続・翻訳・字幕アウトライン試験はこのランチャーを使用しません。
試験ごとの確認範囲は[Qtテスト](qt-tests.md)を参照してください。

## 起動時の検証と後片付け

[ランチャー](../scripts/run-gui-tests.py)はGPUデバイスへのアクセス権と必要なコマンド、
既存の音声ソケットを検出します。PipeWire接続と仮想出力の送受信を確認してから、
固有の一時ディレクトリーに画面とD-Busを起動します。ホストの`DISPLAY`、
`WAYLAND_DISPLAY`、D-Bus接続先は引き継ぎません。音声は検出した接続先と
自分の仮想出力を明示し、既存のクライアント設定や音声ルーティング変数は引き継ぎません。

テストコマンドを開始する前に、次を検証します。

- 専用D-Busへの接続。
- PipeWireのPulseAudio互換サーバーであること。
- 自分の名前・所有モジュールに一致する出力がPipeWireのnull sinkであること。他の出力は許可します。
- GStreamerの`pulsesink`へ送ったテスト音を、仮想出力のmonitorから取得できること。
- Xwaylandが`-displayfd`で通知した専用表示先への接続と、Openboxの起動完了。
- WestonとXwaylandの両方が実GPUで描画していること。

録音開始待ちは、自分のmonitorへ接続した録音プロセスを確認します。
他のアプリケーションの録音ストリームでは成功扱いにしません。
検証に失敗した場合はテストを開始しません。実行中も仮想出力と音声ソケットを定期確認し、
消失・置き換えや専用画面サーバーの終了を検出した場合はテストを停止します。
正常終了・失敗・Ctrl+C・SIGTERMで、起動したプロセス群と一時ソケット、
自分が作った仮想出力を片付けます。音声サーバー本体は停止しません。
モジュール番号だけで削除せず、固有のsink名とモジュール種類を照合します。
接続不能などで削除を確認できない場合はエラーになります。SIGKILLやマシン停止は
後片付けを実行できないため、残った`nagametv_test_`出力はサーバー側での確認が必要です。
テストコマンドの終了コードは呼び出し元に返します。

サーバーとGPU検証のログは`build/gui-tests/session-*/`に保存します。
テスト自身の標準出力・標準エラーは呼び出し元へそのまま出力します。

プロセス管理・接続先の分離・失敗時の停止処理は機器なしで検証できます。

```sh
python3 scripts/test-gui-session.py
```

## 検証環境

2026-09-20: 移行後の管理処理テスト23件とPython／シェルの構文確認が成功しました。
パッケージ切り替えも`apt-get -s`で依存解決を確認しました。
`workshop refresh`後、PipeWire 1.6.2／WirePlumberの自動起動、仮想出力へのテスト音の
送信とmonitorからの取得、終了時の仮想出力削除と音声サービスの継続を確認しました。
二つの音声セッションを同時に保持し、一方の出力削除後も他方を維持できること、
両セッション終了後にそれぞれの仮想出力が残らないことも確認しました。
`pactl`のJSONではmonitorは名前、録音元は数値IDで表されるため、source一覧で対応付けます。
moduleのJSONには番号がないため、削除時の番号は`list short modules`から取得します。
短いテスト音を確実に取得するため、録音側には20msの要求レイテンシーを指定します。
録音したPCMは各セッションの`audio-probe.raw`に残します。

更新直後はNVIDIA EGLプラットフォームの`.so.1`リンク欠落によりXwaylandが
`llvmpipe`へ切り替わったため、テストを停止しました。`ldconfig`で修復後、
WestonとXwaylandの両方でNVIDIA GeForce RTX 4070 Ti（595.84）の描画と
音声loopbackを確認し、`--check`が成功しました。再発防止のためSDKフックにも反映しています。
`bash scripts/test-startup.sh`も全体が成功しました。初回・保存済み起動、自動再生と
環境変数の上書き、番組表、録画再生、タイムシフト、正常終了を確認しています。
検証中に見つかった既存の番組表テストの競合は、表示対象チャンネルの初期更新を
待ってから番組を選択し、ダイアログの待機条件を常に真偽値にする修正で解消しました。
終了後にテスト用sink／moduleが残らず、既存出力の既定設定・音量・ミュートが
維持され、PipeWireサービスが継続していることを確認しました。
Fedora上での実行は未検証です。
以下は移行前のPulseAudioをテストごとに起動していた構成での記録です。

2026-09-18にUbuntu 26.04のWorkshop、Weston 14.0.2、Xwayland 24.1.10、
NVIDIA GeForce RTX 4070 Ti（ドライバー595.84）で、WestonとXwaylandの
実GPU描画と仮想音声の送受信を確認しました。他のGPU・ドライバーの組み合わせは未検証です。

起動・録画再生・タイムシフト、スクリーンショット、字幕描画、ポインター、
ホイール、Portalダイアログ、動画部品の試験と、QML全体の229件が通っています。
管理処理の機器不要テストは8件成功しています。専用セッションの同時起動と、
SIGINT・SIGTERM・コンポジター終了時のテスト停止および後片付けも確認しています。

## 参照仕様

- [Westonのバックエンドとレンダラー](https://wayland.pages.freedesktop.org/weston/toc/running-weston.html)
- [kiosk-shellの全画面制約](https://wayland.pages.freedesktop.org/weston/toc/kiosk-shell.html)
- [Ubuntu 26.04のWeston・Xwayland起動仕様](https://manpages.ubuntu.com/manpages/resolute/man1/weston.1.html)
- [PipeWireのPulseAudio互換サーバー](https://docs.pipewire.org/page_module_protocol_pulse.html)
- [PipeWireのnull sink](https://pipewire.pages.freedesktop.org/pipewire/page_pulse_module_null_sink.html)
- [WirePlumberの出力選択とフォールバック制御](https://pipewire.pages.freedesktop.org/wireplumber/policies/linking.html)
