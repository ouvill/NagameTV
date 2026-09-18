# Flatpakパッケージ

アプリIDは`io.github.ouvill.nagametv`、表示名は日本語で「ながめTV」、英語で「NagameTV」。
KDE 6.10ランタイムを使い、QtとGStreamerの実行環境をホストOSから分離する。

## インストール

Flatpakを導入したLinuxで、配布されたファイルを指定する。

```sh
flatpak install --user ./nagametv-0.1.0-x86_64.flatpak
flatpak run io.github.ouvill.nagametv
```

アプリ一覧からも起動できる。接続先のMirakurun URLは設定画面で指定する。
必要なランタイムはFlathubから取得するため、初回インストールにはネットワーク接続が必要。
`.flatpak`ファイルにKDEランタイム全体は含めない。

新しいパッケージへの更新も`flatpak install --user ./新しいファイル.flatpak`で行う。
このファイル配布にはアプリ用の更新サーバーを設定していないため、
`flatpak update`だけでアプリ本体の新しい版を取得することはできない。

削除する場合:

```sh
flatpak uninstall --user io.github.ouvill.nagametv
```

## パッケージを作る

必要なツールはFlatpak、flatpak-builder、elfutils、Python 3.11以降。
Rust、Qt、libclangなどのコンパイラー・SDKはFlatpak側で用意する。

Ubuntuのビルド用環境なら:

```sh
sudo apt install flatpak flatpak-builder elfutils python3
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
./scripts/build-flatpak.sh
```

出力先は`build/flatpak/`。パッケージ名はCargo.tomlのバージョンとビルド環境のCPUアーキテクチャから決まる。
同じディレクトリーにSHA-256ファイル、ローカルOSTreeリポジトリー`repo/`、ビルド結果`app/`を作る。
ホストOS側へのQtやGStreamerのインストールは不要。初回はSDKを含む数GBのダウンロードが発生する。
並列数は`FLATPAK_BUILD_JOBS=4 ./scripts/build-flatpak.sh`のように指定できる。

マニフェストは`packaging/flatpak/io.github.ouvill.nagametv.json`。
作業ツリーのソースをビルドするため、パッケージ化する前のコミットは必須ではない。
以前のビルド生成物や使っていないcrateの作業ディレクトリーは持ち込まない。

## 依存関係

- Qtは`org.kde.Platform//6.10`を使用し、対応する`org.kde.Sdk`でビルドする。
- ランタイムにない`qml6glsink`はGStreamer Good Plug-ins 1.26.11から追加ビルドする。
  そのほかの再生プラグインとコーデックはランタイムとその拡張を使用する。
- libaribcaptionとtsreadexはリポジトリーのサブモジュールと同じ固定コミットから静的リンクする。
- Rust依存はCargo.lockのバージョンとSHA-256を使って事前取得し、コンパイル時の通信を無効にする。
  ローカルcrateとqt-build-utilsのパッチもソースとして含める。

Cargo.lockを変更したら、次を実行して`cargo-sources.json`も更新する。
ビルドスクリプトは更新漏れを検出すると停止する。

```sh
python3 scripts/flatpak-cargo-sources.py
```

ランタイムの更新はFlatpakが管理する。同じブランチの修正更新でQtなどのパッチ版は変わるため、
QtやGStreamerを更新した際はパッケージの再ビルドと再生確認を行う。

## サンドボックス

スクリーンショットをダイアログなしで保存できるよう、既定の保存先だけに
`--filesystem=xdg-pictures/nagametv:create`で書き込み・作成を許可する。
設定画面で別の保存先を選ぶ場合はQtのフォルダーダイアログを使い、選択先へ書き込めることを確認して保存する。
録画ファイル・保存先の選択はQtのPortal連携を使用する。ネイティブ版の
「Portalが利用できない場合はQt Quick製ダイアログを使う」処理はFlatpakには適用しない。
Flatpakのサブディレクトリー権限については[ファイルアクセスの仕様](https://docs.flatpak.org/en/latest/sandbox-permissions.html#filesystem-access)を参照。

録画のドラッグ＆ドロップは、送信元が提供するFileTransfer Portalの転送キーを受け取り、
ドキュメントポータルが公開したパスを読み込む。初回設定画面でも同じ処理を使う。
GNOME/GTK系・KDE系が使用する標準MIME形式と、旧GTKのMIME形式を受け付ける。
[KDE系の送信元](https://invent.kde.org/frameworks/kcoreaddons/-/blob/master/src/lib/io/kurlmimedata.cpp)
はドロップ終了時に転送を閉じるため、受領通知の前にパスを取得する
（Portalの応答待ちは最大1秒）。録画本体の検査は非同期で行い、キャンセルに対応する。
一度に開くファイルは1件で、`.ts`と`.m2ts`に対応する。
送信元のファイルマネージャーがPortal転送に対応せず、サンドボックス外のパスだけを渡す場合は、
「TSファイルを開く」（Ctrl+O）から選択する。
[FileTransfer Portalの仕様](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.FileTransfer.html)
に従い、この方法で受け取ったファイルのアクセス権はアプリのセッション中だけ有効となる。

| 権限 | 用途 |
| --- | --- |
| network | MirakurunとNX-Jikkyoへの接続 |
| x11 / ipc | X11またはXWaylandでのウィンドウ表示 |
| dri | OpenGLでの映像描画 |
| pulseaudio | PulseAudioまたはPipeWireのPulseAudio互換サーバーへの音声出力 |
| xdg-pictures/nagametv:create | スクリーンショットの既定保存先の作成・書き込み |

現在のQt/GStreamerの描画互換設定に合わせ、パッケージは`QT_QPA_PLATFORM=xcb`で起動する。
WaylandセッションではXWaylandが必要。ネイティブWaylandを検証済みとして扱わない。
ホームディレクトリー全体やチューナーデバイスへのアクセスは付与しない。

設定はFlatpak専用の`~/.var/app/io.github.ouvill.nagametv/config/nagametv/settings.toml`へ保存する。
通常版の設定を自動で取り込む処理は追加していない。
引き継ぐ場合は両方のアプリを終了してから、既存の設定をこの場所へコピーする。
ログはQtの標準保存先を利用し、設定画面の「ログフォルダーを開く」から参照する。
フォルダーを開くには、ホスト側のxdg-desktop-portalとデスクトップに対応するバックエンドが必要。

## 動作確認

2026-09-13にx86_64向けの単一ファイルを生成し、そのファイルからユーザー領域へインストールして確認した。
使用した環境はQt 6.10.3、GStreamer 1.26.11、NVIDIAドライバー595.84。

- サンドボックス内でのNVIDIA GPUによるOpenGL描画。
- ローカルHTTPサーバーからのMPEG-TS再生（MPEG-2 Video / AAC、640×360、30 fps）。
- アプリのPulseAudio出力ストリームの動作。音声データはテスト用の無音を使用。
- 番組一覧と更新イベントの受信、日本語UI・番組名の表示、再生設定と動画統計の操作、正常終了。
- デスクトップ用メタデータ、同梱プラグインの読み込み、共有ライブラリーの解決。

確認用コンテナーはホストのPulseAudioブリッジに接続した。
デスクトップポータルが起動していないため、ログフォルダーを開く操作は未検証。
実放送のARIB字幕、日本語IME、NX-Jikkyoへの投稿、ネイティブWaylandは今回の検証には含めていない。

参考: [Flatpakの単一ファイル配布](https://docs.flatpak.org/en/latest/single-file-bundles.html)、
[サンドボックスの権限](https://docs.flatpak.org/en/latest/sandbox-permissions.html)、
[Qtのポータル対応](https://docs.flatpak.org/en/latest/portals.html#portal-support-in-qt-and-kde)。
