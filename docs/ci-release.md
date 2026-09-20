# CIとリリース

[GitHub Actions](../.github/workflows/ci.yml)でLinux x86_64向けのテストと
AppImage／Flatpak／Ubuntu 24.04・26.04用debのビルドを実行する。`main`へのpush、pull request、`v*`タグのpush、
Actions画面の手動実行が対象。配布ファイルと個別のSHA-256をActions artifactとして14日間保存する。

## CIの確認範囲

| ジョブ | 確認する内容 |
| --- | --- |
| Release metadata | Cargo.toml・Cargo.lock・AppStreamのバージョン、FlatpakのCargo依存一覧とサブモジュールの固定コミット、リリース処理の回帰テスト、actionlint・ShellCheck |
| Tests, AppImage and deb (Ubuntu 24.04, x86_64) | アプリのRust書式、Rustテスト、独立crateのテスト、Pythonテスト、機器不要のQt試験、AppImageの生成と全ELFのglibc上限検査、24.04用debの生成・導入・依存解決・削除 |
| deb (Ubuntu 26.04, amd64) | Ubuntu 26.04のQt/GStreamerでビルドし、debの生成・導入・依存解決・削除 |
| Flatpak (x86_64) | KDE SDK内でのオフラインコンパイル、FlatpakとSHA-256の生成 |

AppImageとテストは[配布用Dockerfile](../packaging/appimage/Dockerfile)の`ci`ステージを使う。
Ubuntu 24.04、Rust 1.98.1、Qt 6.8.3、GStreamer 1.24を共用し、テスト用にrustfmtを追加する。
FlatpakはマニフェストのKDE 6.10 SDKを使う。26.04用debは[専用Dockerfile](../packaging/deb/Dockerfile)でUbuntuのQt/GStreamerを使う。
GitHubの実行ホストはいずれも`ubuntu-24.04`。
並列コンパイルは2件に制限し、Dockerレイヤー、Cargoのダウンロード、配布用ツール、
Flatpakのビルド状態をキャッシュする。ホストのコンパイル済みCargoキャッシュは取り込まない。

Rustテストはアプリに加えて`viewer-comments`・`viewer-epg-events`（どちらも`network`付き）、
`viewer-diagnostics`・`viewer-remote`・`tsreadex`を各lockfileの`--locked`で実行する。
Qt試験は接続・翻訳・字幕アウトラインが対象。
CIでは`NAGAMETV_TEST_PROFILE=release`でQt試験もリリースプロファイルを使い、
デバッグ用依存ライブラリーの二重ビルドを避ける。通常の開発用スクリプトの既定は`dev`。
debの導入試験は開発パッケージのない対象Ubuntuコンテナーで行い、
全ELFの依存解決・引数解析（Qt初期化前に終了）・ファイル整合性・削除を検証する。
画面、GPU、音声機器は使用せず、Dockerに機器やホストセッションを渡さない。
GUI・再生試験は[実GPUを使う専用環境](gui-test-environment.md)で別途実行する。
CI成功だけでは画面表示や再生の動作確認を済ませたことにはならない。

## ローカルで同じテストを実行する

Dockerが利用できるLinux x86_64環境で実行する。

```sh
git submodule update --init --recursive
docker build --target ci --tag nagametv-ci:local packaging/appimage
docker run --rm --user "$(id -u):$(id -g)" \
  --mount "type=bind,src=$PWD,dst=/project" \
  --env CARGO_HOME=/project/build/ci/cargo-home \
  --env CARGO_TARGET_DIR=/project/build/ci/cargo \
  --env CARGO_BUILD_JOBS=2 \
  nagametv-ci:local bash scripts/ci.sh
```

依存関係を導入済みのネイティブ開発環境では`bash scripts/ci.sh`でも実行できる。
ネイティブ実行の既定の出力先は`build/ci-native/cargo`。
Docker版とネイティブ版で同じ`CARGO_TARGET_DIR`を共有しないこと。
パッケージ単体の作成方法は[AppImage](appimage.md)・[Flatpak](flatpak.md)・[deb](deb.md)を参照。

## リリースを作る

1. `rust/Cargo.toml`のアプリバージョンを更新し、`rust/Cargo.lock`も更新する。
   `packaging/linux/io.github.ouvill.nagametv.metainfo.xml`の`releases`の先頭に
   同じバージョンとリリース日を追加する。
2. `python3 scripts/flatpak-cargo-sources.py`で依存一覧を更新する。
   サブモジュールを更新した場合はFlatpakマニフェストの`commit`も合わせる。
3. `python3 scripts/check-release-metadata.py --tag v0.1.0`のように予定のタグを検査し、
   変更をコミットして`main`のCI成功を確認する。
4. リリース対象のコミットにタグを付けてpushする。例:

   ```sh
   git tag -a v0.1.0 -m "NagameTV 0.1.0"
   git push origin v0.1.0
   ```

5. タグのコミットでCIを再実行し、全ジョブ成功後にGitHub Releaseの**下書き**を作る。
   AppImage・Flatpak・24.04用deb・26.04用debとSHA-256の計8ファイルを添付し、変更履歴を自動生成する。
   `v0.2.0-rc.1`のようなタグはprereleaseとして扱う。
6. 下書きから配布ファイルを取得して専用環境・対象OSでGUIと再生を確認し、
   リリースノートを編集してGitHub上で公開する。自動公開はしない。

タグは`v`とCargo.tomlのバージョンが完全一致する必要がある。
いずれかのビルドや検査が失敗した場合、下書き作成へ進まない。
同じタグのActionsを再実行すると既存の下書きの添付ファイルを更新する。
公開済みリリースのファイルは上書きせず停止する。
artifactの保存期限を過ぎた場合は、ビルドを含む全ジョブを再実行する。
Actionsの手動実行は検証・artifact生成用で、下書きはタグpushの実行でのみ作る。

## GitHub側の設定と保守

通常のCIジョブは`contents: read`のみ。リリースジョブだけに`contents: write`を付け、
標準の`GITHUB_TOKEN`で下書きを作る。追加のPATや配布用秘密鍵は不要。
リポジトリー／OrganizationのポリシーでActionsと利用するActionが許可されている必要がある。

`main`の保護ルールを設定する場合は、上記4つのCIジョブを必須チェックに指定する。
リリースジョブはpull requestで実行しないため、必須チェックには指定しない。
ワークフローの追加自体ではGitHub側の保護ルールは変更されない。

外部ActionはコミットSHAで固定し、[Dependabot](../.github/dependabot.yml)で毎週更新を提案する。
UbuntuベースイメージのdigestもDependabotの対象。Rust・Qt・GStreamer・actionlintの
バージョンとSHA-256は対応するDockerfile／スクリプト／ワークフローで明示的に更新する。
Flatpakの同一SDKブランチ内の更新とUbuntuのapt更新は可変のため、ビット単位の再現性は保証しない。

設定変更時の軽量な確認:

```sh
python3 scripts/test-release.py
python3 scripts/check-release-metadata.py
python3 scripts/flatpak-cargo-sources.py --check
bash -n scripts/ci.sh scripts/create-release-draft.sh scripts/build-appimage.sh scripts/build-deb.sh scripts/test-deb.sh
actionlint
```

リリース処理のテストは一時GitリポジトリーとローカルのGitHub CLI応答を使う。
バージョン不一致、破損・欠落した成果物、API失敗、下書きの再実行、公開済みリリースの保護を
ネットワーク通信やGitHubへの書き込みなしで確認する。

参照: [GitHub Actionsのartifact](https://github.com/actions/upload-artifact)、
[GitHub CLIのRelease作成オプション](https://cli.github.com/manual/gh_release_create)。
