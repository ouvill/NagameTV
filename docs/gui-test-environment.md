# GUIテストの専用環境

LinuxのGUIテストは、Workshop内にテスト専用の画面・入力・音声・D-Busセッションを
作成します。ホストでのマウス操作やフォーカス移動、音量・出力先の変更による
テストへの干渉を防ぎます。ホストから共有する機器はGPUです。

| 資源 | 使用する環境 |
|---|---|
| 描画 | ホストから共有した実GPU。ソフトウェア描画は拒否します。 |
| 画面・入力 | WestonのheadlessバックエンドとGLレンダラー、専用XwaylandとOpenbox。1920×1080の仮想画面です。 |
| 音声 | 専用PulseAudioの`module-null-sink`。スピーカーへ音を出しません。 |
| セッションバス | 専用`dbus-daemon`。ホストのPortalには接続しません。 |

これは明示的に選択したテスト環境です。GPUが利用できない場合の代替処理ではありません。
物理モニター、GNOME/KDE固有のウィンドウ操作、実音声デバイスの遅延や聴取品質は
この環境では確認できません。GPUの処理能力はホストの作業と共有します。

Xwaylandはrootfulモードで起動し、X11ウィンドウの管理をOpenboxに任せます。
物理入力機器のないWestonのrootless構成では、ダイアログを閉じた後に親画面へ
フォーカスが戻らない場合があります。専用のX11ウィンドウマネージャーにより、
キーボード操作やダイアログの開閉をテスト内で完結させます。

## Workshopの準備

[Workshop定義](../.workshop/dev.yaml)のGUI用接続は`gui:gpu → system:gpu`のみです。
Weston、Xwayland、PulseAudioなどは[プロジェクトSDK](../.workshop/nagametv/hooks/setup-base)で
導入します。既存Workshopへの定義変更の反映方法は[開発手順](development.md)を参照してください。
更新時には旧GUI SDKが設定した`PULSE_SERVER=tcp:127.0.0.1:4713`のexportも除去します。

ホスト側で環境の検証だけを実行する場合:

```sh
workshop run -- gui-test --check
```

Workshop内では次のコマンドを使います。

```sh
python3 scripts/run-gui-tests.py --check
```

## テストの実行

通常のGUIテストスクリプトは専用セッションを自動的に起動します。表示先の環境変数を
手動で設定する必要はありません。

```sh
bash scripts/test-screenshot.sh
bash scripts/test-startup.sh
bash scripts/test-pointer-activity.sh
```

ホストから実行する場合はWorkshop内で起動してください。

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

[ランチャー](../scripts/run-gui-tests.py)はGPUデバイスへのアクセス権と必要なコマンドを
検出した後、固有の一時ディレクトリーに各サーバーを起動します。ホストの`DISPLAY`、
`WAYLAND_DISPLAY`、音声・D-Busの接続先は引き継ぎません。PulseAudioは指定した
仮想出力だけを読み込み、音声機器の自動検出やホストへの接続を行いません。

テストコマンドを開始する前に、次を検証します。

- 専用D-Busへの接続。
- PulseAudioの出力が専用null sinkだけであること。
- GStreamerの`pulsesink`へ送ったテスト音を、仮想出力のmonitorから取得できること。
- Xwaylandが`-displayfd`で通知した専用表示先への接続と、Openboxの起動完了。
- WestonとXwaylandの両方が実GPUで描画していること。

検証に失敗した場合はテストを開始しません。実行中にサーバーが終了した場合もテストを停止します。
正常終了・失敗・Ctrl+C・SIGTERMで、起動したプロセス群と一時ソケットを片付けます。
テストコマンドの終了コードは呼び出し元に返します。

サーバーとGPU検証のログは`build/gui-tests/session-*/`に保存します。
テスト自身の標準出力・標準エラーは呼び出し元へそのまま出力します。

プロセス管理・接続先の分離・失敗時の停止処理は機器なしで検証できます。

```sh
python3 scripts/test-gui-session.py
```

## 検証環境

2026-09-18にUbuntu 26.04のWorkshop、Weston 14.0.2、Xwayland 24.1.10、
NVIDIA GeForce RTX 4070 Ti（ドライバー595.84）で、WestonとXwaylandの
実GPU描画と仮想音声の送受信を確認しました。他のGPU・ドライバーの組み合わせは未検証です。

起動・録画再生・タイムシフト、スクリーンショット、字幕描画、ポインター、
ホイール、Portalダイアログ、動画部品の試験と、QML全体の229件が通っています。
管理処理の機器不要テストは8件成功しています。専用セッションの同時起動と、
SIGINT・SIGTERM・コンポジター終了時のテスト停止および後片付けも確認しています。

## 参照仕様

- [Westonのバックエンドとレンダラー](https://wayland.pages.freedesktop.org/weston/toc/running-weston.html)
- [Ubuntu 26.04のWeston・Xwayland起動仕様](https://manpages.ubuntu.com/manpages/resolute/man1/weston.1.html)
- [PulseAudioのnull sinkとmonitor](https://wiki.freedesktop.org/www/Software/PulseAudio/Documentation/User/Modules/#module-null-sink)
