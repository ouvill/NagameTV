# AppImageパッケージ

Linux x86_64向けに、アプリ本体・Qt/QML・GStreamerの再生プラグインを
1つの`.AppImage`にまとめる。配布用ビルドの基準は**Ubuntu 24.04 / glibc 2.39**。
WorkshopのUbuntu 26.04とは別のDocker環境でビルドする。
GitHub Actionsでの自動ビルドと配布は[CIとリリース](ci-release.md)を参照。

## パッケージを作る

ホストにはLinux x86_64、Bash、Git、利用可能なDockerデーモンが必要。
Canonical Workshopでは既存の`docker-ce` SDKを使う。
[Dockerfile](../packaging/appimage/Dockerfile)がUbuntu 24.04、Rust 1.98.1、
Qt 6.8.3の公式SDK、UbuntuのGStreamer 1.24を用意する。
Ubuntu 24.04標準のQtはアプリの要件を満たさないため使用しない。
Qt内部APIを使う`qml6glsink`は、同じQt SDKとGStreamerに対してソースからビルドする。
ホストのQt/GStreamer、Cargoのコンパイル済みキャッシュは持ち込まない。

リポジトリーのルートで実行する。

```sh
git submodule update --init
./scripts/build-appimage.sh
```

出力先:

```text
build/appimage/nagametv-0.1.0-x86_64.AppImage
build/appimage/nagametv-0.1.0-x86_64.AppImage.sha256
```

バージョンは`rust/Cargo.toml`から取得する。作業ツリーのソースをCMakeと
`cargo build --release --locked`でビルドするため、コミットは不要。
コンパイル結果とCargoキャッシュは`build/appimage/ubuntu24/`へ分離する。
並列数を変える場合:

```sh
APPIMAGE_BUILD_JOBS=4 ./scripts/build-appimage.sh
```

既にglibc 2.39以下、Qt 6.8以降、GStreamer 1.24以降が揃った環境では、
`--native`でDockerを使わずビルドできる。Bash、CMake、Cargo、pkg-config、
curl、Python 3.11以降、sha256sum、flock、desktop-file-validate、readelf、getconf、fileと、
Qtのqmake・qmlimportscanner・画像プラグイン、
[GStreamerプラグイン一覧](../packaging/appimage/gstreamer-plugins.txt)の全ファイルが必要。
HTTPプラグイン用の`libsoup-3.0-dev`と、GIOのTLSモジュールを含む`glib-networking`も必要。
このモードだけ、ビルドディレクトリーとQtを指定できる:

```sh
APPIMAGE_BUILD_DIR="$PWD/build/appimage-native" APPIMAGE_BUILD_JOBS=4 \
  QMAKE=/path/to/qt/bin/qmake ./scripts/build-appimage.sh --native
```

linuxdeploy・Qtプラグイン・AppImageランタイムは、固定リリースを取得して
SHA-256を確認し、`build/appimage/tools/`にキャッシュする。
初回のDocker環境作成、ツール取得と未取得のCargo依存のダウンロードにはネットワーク接続が必要。
ツールは常に展開実行するため、パッケージ作成にFUSEやマウント権限は不要。
コンパイル・パッケージ作成は表示・GPU・音声機器を使用しない。
Dockerにも表示・音声ソケットやGPUデバイスを渡さない。

## 起動する

```sh
chmod +x ./nagametv-0.1.0-x86_64.AppImage
./nagametv-0.1.0-x86_64.AppImage
```

X11またはXWayland、GPUによるOpenGL描画、PulseAudioまたはPipeWireの
PulseAudio互換サーバーが必要。Qt/GStreamerを利用者の環境に別途インストールする必要はない。
日本語UI用のNoto Sans CJK JP、GPUドライバー、システムの証明書はホスト側で用意する。
ARIB字幕フォントはアプリ本体に組み込まれている。

通常起動はFUSEを使用する。マウントできない環境では、明示的に展開して起動できる。
これはファイルの読み出し方法を変えるだけで、表示・GPU・音声の要件は同じ。

```sh
./nagametv-0.1.0-x86_64.AppImage --appimage-extract
./squashfs-root/AppRun
```

アプリ一覧への登録や自動更新は行わない。
設定はネイティブ版と同じ`~/.config/nagametv/settings.toml`を使う
（`XDG_CONFIG_HOME`で変更可能）。Flatpak版の設定とは別。

### 再生時計を変えて比較する

同じAppImageを一度終了してから、次の2通りで起動する。

```sh
# 通常の時計選択（未指定時もこちら）
NAGAMETV_PLAYBACK_CLOCK=auto ./nagametv-0.1.0-x86_64.AppImage
# 音声出力を保ち、映像と音声の再生時計をSystemClockへ固定
NAGAMETV_PLAYBACK_CLOCK=system ./nagametv-0.1.0-x86_64.AppImage
```

比較方法とログの読み方は[再生時計の比較](audio-output.md#再生時計の比較)を参照。
この指定は設定ファイルには残らない。展開実行でも同じ環境変数を`AppRun`に指定できる。

## 更新・削除

起動中のながめTVをすべて終了してから、更新・削除を行う。
Linuxでは、実行ファイルを削除しても動作中のプロセスが終了するとは限らない。

更新は新しいAppImageへの置き換え、削除は使用していたAppImageファイルの削除で行う。
展開して利用した場合は、その展開先の`squashfs-root`ディレクトリーも削除できる。
アプリ一覧への登録やログイン時の自動起動を自分で設定した場合は、その登録も解除する。

設定・キャッシュも削除する場合は[ネイティブ版の保存データの削除](development.md#ネイティブ版の保存データの削除)を参照。

## 同梱内容と互換性

- CMakeのinstall規則から実行ファイル、デスクトップ情報、アイコン、ライセンスを取り込む。
- linuxdeployのQtプラグインが製品QMLのimportを走査し、Qtライブラリー、QMLモジュール、
  X11・SVG・入力メソッドなどのプラグインを収集する。テスト用QMLは対象に含めない。
- GStreamerは[プラグイン一覧](../packaging/appimage/gstreamer-plugins.txt)のライブラリーと
  plugin scanner、共有ライブラリー依存を同梱する。MPEG-TS、MPEG-2/H.264、AAC、
  インターレース解除、Qtへの映像表示、PulseAudio出力を含む。
- GStreamer 1.24が実行時に読み込むlibsoup 3と、GIOのGnuTLSモジュールも明示的に同梱する。
  これらはELFの通常の依存走査だけでは見つからない。GIOのTLSモジュールも同梱先を参照する。
- AppRunは同梱Qt/GStreamerを参照する。GStreamerのホスト側プラグインを混在させず、
  レジストリーも`~/.cache/nagametv/appimage/`へ分離する
  （`XDG_CACHE_HOME`で変更可能）。
- QtのPortalプラグインはビルド環境に存在すれば収集する。利用できるPortalがなければ、
  ネイティブ版と同じQt Quickダイアログを使う。[選択方針](platform-startup.md)

AppImageはOS全体を同梱しない。glibcとGPUドライバーなどはホスト側を使用するため、
新しいOSでビルドしたファイルが古いOSでも動くとは限らない。
配布ビルドはUbuntu 24.04を基準にし、収集した全ELFファイルの要求バージョンを
`readelf`で検査する。glibc 2.39を超える要求や非公開のglibc ABI依存があれば、
AppImageを出力する前に停止する。この検査だけでGUI・再生やGPUドライバーの互換性までは保証しない。
CPUアーキテクチャのクロスビルドは対象外。

### Ubuntu 24.04で`GLIBC_2.43 not found`になる場合

2026-09-19までにUbuntu 26.04上で作ったAppImageは、同梱Qt・GLib・GnuTLSなどが
glibc 2.43を要求するため、glibc 2.39のUbuntu 24.04では起動できない。
AppImageにまとめてもシステムglibcへの依存は残る。
上記のUbuntu 24.04ビルドで生成し直したAppImageに置き換える。

参考: [AppImageの互換性指針](https://docs.appimage.org/reference/best-practices.html)、
[linuxdeployのQtプラグイン](https://github.com/linuxdeploy/linuxdeploy-plugin-qt)、
[GStreamerの検索パス](https://gstreamer.freedesktop.org/documentation/gstreamer/running.html)。

## 検証

glibc検査の回帰テストは、Cコンパイラーとreadelfがある環境で実行できる。
実際のELF共有ライブラリーを作り、許容バージョン・新しすぎる依存・非公開ABI・
壊れたELFを確認する。表示・GPU・音声は使用しない。

```sh
python3 scripts/test-appimage-glibc.py
python3 scripts/check-appimage-glibc.py /path/to/squashfs-root --max-glibc 2.39
```

2026-09-17にUbuntu 26.04 / x86_64、Qt 6.10.2、GStreamer 1.28.2で生成を確認した。

- AppImageのSHA-256、展開、デスクトップ情報の検証。
- 同梱した309個のELFファイルの共有ライブラリー解決、18個のGStreamerプラグインとscannerの存在。
- 主要なGStreamer要素10種類の登録と、同梱プラグインによるMPEG-2/AACのTS試験素材のデコード完了。
  この試験はCPUのみを使い、映像・音声を出力しない。
- X11、NVIDIA GPUのOpenGL、PulseAudio出力を検証した環境で、展開したAppRunから製品画面を表示。
  設定・キャッシュは一時ディレクトリーへ分離し、QMLの読み込みエラーがないことを確認した。

別のOS、FUSEによる通常起動、実放送の映像・音声出力、Portalは今回の確認に含めていない。
この生成物を古いUbuntuなどでも動作確認済みとして扱わない。

2026-09-18にはWorkshopのSDK依存を補完して再ビルドし、SHA-256と、追加したQt Portalプラグインの
同梱・共有ライブラリー解決を確認した。実際のPortalダイアログ操作は引き続き未検証。

2026-09-19には再生時計を比較できる版を生成し、SHA-256、展開、322個のELFの依存解決、
主要な再生要素の登録、同梱プラグインによるMPEG-2/AAC素材のCPUデコードを確認した。
この生成物の同梱ライブラリーは**glibc 2.43以上**を要求する。AppImageでもこの条件は残る。
検証済みの専用GPU・仮想音声環境で、展開した製品AppRunを`auto`と`system`で起動した。
HTTPで実時間配信する合成TSを再生し、映像の描画、実際の時計名、PulseAudioの有効な出力と
monitor上の音声サンプル、停止・再生、正常終了を両条件で確認した。
この試験では`--features=none`を指定し、字幕・EPG・コメントは対象外とした。
物理画面までの遅延、実スピーカーでの音ずれ、長時間視聴、他OS、FUSE起動は未検証。

同日、Ubuntu 24.04で`GLIBC_2.43 not found`となる報告を再現し、配布ビルドを
Ubuntu 24.04 / Qt 6.8.3 / GStreamer 1.24へ切り替えた。切り替え後の生成物では次を確認した。

- SHA-256と展開、311個のELFの要求がglibc 2.39以下であること。
- QtをインストールしていないUbuntu 24.04コンテナーで、全ELFの依存解決と、
  実行ファイルの引数解析までの起動（Qt初期化前に終了）。旧版のglibcエラーは解消した。
- 同コンテナーで主要なGStreamer要素10種類の登録、MPEG-2/AAC素材のCPUデコード、
  テスト用CAを信頼したローカルHTTPS取得。libsoupとGIOのTLSモジュールも同梱物を使用した。
- Ubuntu 26.04のWorkshop内の専用GPU・仮想音声環境で、最終AppImageを展開したAppRunから
  `auto`・`system`両方の時計設定で映像描画・音声サンプル・停止／再開・正常終了を確認した。
  この再生試験も`--features=none`を使用した。
- glibc検査の回帰テスト6件（WorkshopとUbuntu 24.04の両環境）。

Ubuntu 24.04の実デスクトップでの画面・音声、物理機器、FUSE起動は引き続き未検証。
Ubuntu 24.04でのライブラリー互換性検査と、WorkshopでのGUI再生検査を区別する。

パッケージ作成後はSHA-256と展開内容を機器なしで確認できる。

```sh
cd build/appimage
sha256sum --check nagametv-0.1.0-x86_64.AppImage.sha256
./nagametv-0.1.0-x86_64.AppImage --appimage-extract
desktop-file-validate squashfs-root/io.github.ouvill.nagametv.desktop
ldd squashfs-root/usr/bin/nagametv
```

画面表示と再生の確認は、表示サーバー・GPU・音声出力を検出・検証してから行う。
機器がない場合はその試験を停止する。ソフトウェア描画やダミー音声へ自動で切り替えない。
配布前には生成物を対象OSで起動し、ライブ視聴・TS録画・字幕・音声・ファイル選択を確認する。
