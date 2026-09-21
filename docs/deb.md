# Ubuntu debパッケージ

Linux amd64向けにUbuntu 24.04用と26.04用を作る。
GitHub Actionsでは両方を生成し、AppImageやFlatpakとともに
`main`の最新Pre-releaseまたはバージョンタグのリリース下書きへ添付する。
[CIとリリース](ci-release.md)を参照。

| 対象 | Qt／GStreamer | 配置 |
| --- | --- | --- |
| Ubuntu 24.04 | AppImageと同じQt 6.8.3・GStreamer・QML・プラグインを同梱 | `/opt/nagametv`、起動用の`/usr/bin/nagametv` |
| Ubuntu 26.04 | Ubuntuのパッケージをaptで導入 | `/usr/bin/nagametv`、`/usr/share/` |

24.04標準のQt 6.4ではアプリが必要とするQt 6.8以上を満たせないため、
専用ディレクトリーに同梱する。OSのQtやGStreamerを置き換えない。
同梱ライブラリーの更新はNagameTVの再ビルド・更新で行う。
26.04用はOSのライブラリー更新を利用する。
どちらもUbuntu公式リポジトリーへの収録やaptリポジトリーの提供は行わない。

## インストール・更新・削除

使用中のUbuntuに合うファイルをダウンロードし、そのディレクトリーで実行する。

```sh
# Ubuntu 24.04
sha256sum --check nagametv_0.1.0-1ubuntu24.04_amd64.deb.sha256
sudo apt install ./nagametv_0.1.0-1ubuntu24.04_amd64.deb

# Ubuntu 26.04
sha256sum --check nagametv_0.1.0-1ubuntu26.04_amd64.deb.sha256
sudo apt install ./nagametv_0.1.0-1ubuntu26.04_amd64.deb
```

必要な依存パッケージもaptが導入する。Ubuntuの`universe`を有効にしておく。
インストール後はアプリ一覧、または`nagametv`コマンドで起動する。
更新時も新しいdebを`apt install ./ファイル名.deb`で導入する。
Ubuntuを24.04から26.04へ更新したら、26.04用のdebも導入する。
同じ`nagametv`パッケージとして置き換わり、24.04用の同梱ファイルは削除される。

```sh
sudo apt remove nagametv
```

ユーザー設定はネイティブ版・AppImage版と共通の`~/.config/nagametv/`。
削除・更新でユーザー設定を消さない。Flatpak版とは設定先が異なる。

GUIと再生にはデスクトップ、実GPUによるOpenGL、利用できる音声サーバーが必要。
24.04用はAppImageと同じX11／XWaylandを使用する。
26.04用はシステムQtのWayland／X11を使用する。
debの導入・起動スクリプトは音声サーバーやデスクトップ設定を変更しない。

## パッケージを作る

Linux x86_64、Docker、Gitを用意して、リポジトリールートで実行する。
ビルドに表示・GPU・音声機器は使わず、Dockerに機器やホストセッションを渡さない。

```sh
git submodule update --init --recursive
./scripts/build-deb.sh 24.04
./scripts/build-deb.sh 26.04
```

24.04用は[AppImageのDocker環境](../packaging/appimage/Dockerfile)で
AppImageを生成し、その検証済み内容を展開する。FUSEは不要。
26.04用は[専用Dockerfile](../packaging/deb/Dockerfile)でネイティブビルドする。
両方とも`NAGAMETV_DISTRIBUTION=ON`を使用し、評価用featureは配布できない。
Cargo／コンパイルキャッシュはOSごとに分離する。
`DEB_BUILD_JOBS=4`のように並列数を指定できる。既定は8、CIでは2。

出力例:

```text
build/deb/ubuntu24.04/nagametv_0.1.0-1ubuntu24.04_amd64.deb
build/deb/ubuntu24.04/nagametv_0.1.0-1ubuntu24.04_amd64.deb.sha256
build/deb/ubuntu26.04/nagametv_0.1.0-1ubuntu26.04_amd64.deb
build/deb/ubuntu26.04/nagametv_0.1.0-1ubuntu26.04_amd64.deb.sha256
```

バージョンはCargo.tomlから取得し、Debianのrevisionと対象Ubuntuを付ける。
`0.2.0-rc.1`は`0.2.0~rc.1-1ubuntu24.04`のように変換し、正式版より前に並べる。
成果物とチェックサムがすべて揃わない場合、リリースの作成や更新処理へは進まない。

対象と同じUbuntuで、Dockerfile記載のツールが導入済みなら`--native`も使える。
24.04用ではQt公式SDKと専用ビルドのqml6glsinkも必要なため、Dockerを推奨する。
26.04のWorkshopには[プロジェクトSDK](../.workshop/nagametv/hooks/setup-base)が必要なツールを含む。

```sh
./scripts/build-deb.sh 26.04 --native
# 26.04の既存コンパイルキャッシュを使う場合
DEB_BUILD_DIR="$PWD/build" ./scripts/build-deb.sh 26.04 --native
# CIなどで生成直後のAppImageを再利用する場合（Ubuntu 24.04内）
./scripts/build-deb.sh 24.04 --native \
  --appimage "$PWD/build/appimage/nagametv-0.1.0-x86_64.AppImage"
```

`--appimage`は対応するSHA-256、ファイル名のバージョンとアーキテクチャ、
全ELFのglibc上限2.39を確認する。任意の過去の成果物が現在のソースと一致することまでは
確認しないため、通常は省略し、CIでは同じジョブで生成したものだけを渡す。

共有ライブラリー依存は対象OS内で`dpkg-shlibdeps`から生成する。
24.04では同梱するライブラリーだけを自己依存として除外し、外部依存の検査は省略しない。
26.04のQML・画像・再生プラグインなど動的に読み込む依存は
[明示的な一覧](../packaging/deb/runtime-dependencies-26.04.txt)で補う。

## 検証

Debianパッケージのバージョン命名規則やアップグレード順序、およびリリース処理のガードは、ホスト上で回帰テストを実行して確認する。

```sh
python3 scripts/test-release.py
```

パッケージの生成後は、クリーンな対象Ubuntuコンテナーで、aptによる依存解決・導入、
全ELFのリンク解決、アプリの引数解析、デスクトップ情報、削除を確認する。
表示・GPU・音声にはアクセスしない。コンテナー外のパッケージは変更しない。

```sh
bash scripts/test-deb.sh 24.04
bash scripts/test-deb.sh 26.04
```

これらの検査はGUI起動や映像・音声再生の確認を含まない。
配布前のGUI試験は[専用GUI環境](gui-test-environment.md)で資源を検出・検証し、
対象OSでも表示・ライブ視聴・TS録画・字幕・音声・ファイル選択を確認する。

参考: [dpkg-shlibdeps](https://manpages.debian.org/trixie/dpkg-dev/dpkg-shlibdeps.1.en.html)、
[Debianのバージョン規則](https://www.debian.org/doc/debian-policy/ch-controlfields.html#version)。
