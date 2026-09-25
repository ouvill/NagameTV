# CIとリリース

[GitHub Actions](../.github/workflows/ci.yml)でLinux x86_64向けのテストと
AppImage、Flatpak、Ubuntu 24.04用および26.04用debのビルドを実行する。
`main`へのpush、pull request、`v*`タグのpush、Actions画面の手動実行（workflow_dispatch）が対象。
配布ファイルとそれぞれのSHA-256の計8ファイルをActions artifactとして14日間保存する。
全ジョブ成功後、`main`へのpushでは最新ビルドのPre-releaseを作成または更新し、
`v*`タグのpushではGitHub Releaseの下書きを作成する。pull requestと手動実行では検証とartifact生成のみを行う。

## CIの確認範囲

| ジョブ | 確認する内容 |
| --- | --- |
| Release metadata | Cargo.toml・Cargo.lock・AppStreamのバージョン、FlatpakのCargo依存一覧とサブモジュールの固定コミット、リリース処理の回帰テスト、actionlint・ShellCheck |
| Tests, AppImage and deb (Ubuntu 24.04, x86_64) | アプリのRust書式、Rustテスト、独立crateのテスト、Pythonテスト、本体全QMLの静的検査、UI共通ルールの検査、機器不要のQt試験、AppImageの生成と全ELFのglibc上限検査、24.04用debの生成・導入・依存解決・削除 |
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
EPGStationは[固定版の本体との結合テスト](epgstation.md#固定版の実サーバーとの結合テスト)も必須で実行する。
ホスト側で専用コンテナーを管理し、製品のRustクライアントは既存のビルドイメージで検査する。
録画一覧などの保存済み実応答と新たな応答を比較し、差があれば失敗する。
診断ログは`epgstation-contract` artifactとして14日間保存する。
本体QMLは生成した型情報を使って全ファイルを`qmllint`で検査し、警告0件を合格条件とする。
評価用部品も静的検査に含める。色・寸法・動きの共通化は`check-ui-style.py`で検査する。
CIの`scripts/ci.sh`は共通ランナー`python3 scripts/test.py cpu`を実行する。
Rustはnextestで個別プロセスに分離し、Qt試験は一度ビルドした実行ファイルを共用する。
ローカルとCIの既定はともに`release`で、ビルドと検証は共通ロックで直列化する。
各検査のログと実行結果は、失敗時も`test-results` artifactとして14日間保存する。
詳しい選択方法とログの保存先は[開発手順](development.md#テストと診断)を参照。
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
  --mount type=bind,src=/etc/passwd,dst=/etc/passwd,readonly \
  --mount type=bind,src=/etc/group,dst=/etc/group,readonly \
  --env CARGO_HOME=/project/build/ci/cargo-home \
  --env CARGO_TARGET_DIR=/project/build/ci/cargo \
  --env CARGO_BUILD_JOBS=2 \
  nagametv-ci:local bash scripts/ci.sh
CARGO_HOME=/project/build/ci/cargo-home CARGO_TARGET_DIR=/project/build/ci/cargo \
  CARGO_BUILD_JOBS=2 python3 scripts/epgstation-integration.py --client-image nagametv-ci:local
```

D-Busが数値UID/GIDを解決できるように、ユーザー・グループ情報を読み取り専用で渡す。
画面や音声のソケットは渡さない。ユーザー情報がない場合はコンパイル前に停止する。
Flatpakのホスト側AppStream生成にはSVGローダーも必要で、CIでは`librsvg2-common`を明示的に導入する。

依存関係を導入済みのネイティブ開発環境では`bash scripts/ci.sh`でも実行できる。
ネイティブ実行の既定の出力先は`build/ci-native/cargo`。
Docker版とネイティブ版で同じ`CARGO_TARGET_DIR`を共有しないこと。
パッケージ単体の作成方法は[AppImage](appimage.md)・[Flatpak](flatpak.md)・[deb](deb.md)を参照。

## mainの最新ビルド

`main`へのpushでは、全テストと配布ビルドの成功後に
固定タグ[`latest-build`の公開Pre-release](https://github.com/ouvill/NagameTV/releases/tag/latest-build)を1件作成または更新する。
AppImage、Flatpak、Ubuntu 24.04用および26.04用debと、それぞれのSHA-256の計8ファイルを差し替える。
GitHubの正式版を示すLatestには設定しない。

`latest-build`タグはビルドしたコミットへ移動し、リリースノートにCargo.tomlのバージョンと完全なコミットSHAを記録する。
タイトルと本文は日本語で生成し、本文の先頭に各パッケージとSHA-256への直接ダウンロードリンクを載せる。
Assetsが折りたたまれていても取得できるようにし、対象環境・導入手順・開発ビルドの更新方法も案内する。
本文は[`latest-build-notes.py`](../scripts/latest-build-notes.py)で生成し、ファイル名は配布処理と共通のメタデータを使う。
パッケージ内部のバージョンと配布ファイル名にはCargo.tomlのバージョンを使い、ビルドごとのバージョン更新は不要である。
バージョン更新によってファイル名が変わった場合は、以前の添付ファイルを削除する。

成果物の検証と更新は段階的に進める。ダウンロードした配布ファイルとチェックサムをすべてローカルで検証した後に、
既存のPre-releaseを一時的に下書きへ戻す。タグの移動、ファイルの差し替え、古いファイルの削除が完了した時点で再公開する。
更新中および更新途中の失敗時は公開ダウンロードができない。
更新途中で失敗した場合も下書きのまま保持され、同じワークフロー実行を再実行することで不足ファイルを補完して再公開できる。

ビルドの並行動作と公開順序はジョブ設定で制御する。`main`の各ビルドは独立して実行し、公開ジョブはconcurrencyによって直列化する。
待機ジョブには`queue: max`を指定し、後から完了したビルドによって待機中のジョブが取り消されることを防ぐ。GitHubにおける待機上限は100件である。
公開処理の開始時にGitHub上の`main`の先頭コミットを読み込み、ビルドしたコミットがすでに古い場合は書き込みを行わずに終了する。
これにより、ビルド完了順の前後や古いCIの再実行による巻き戻しを防ぐ。
後続のpushでCIが失敗した場合は、最後に公開されたビルドが保持される。

## バージョンタグからリリースを作る

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

5. タグのコミットでCIを再実行し、全ジョブ成功後にGitHub Releaseの下書きを作成する。
   AppImage・Flatpak・24.04用deb・26.04用debとSHA-256の計8ファイルを添付し、変更履歴を自動生成する。
   `v0.2.0-rc.1`のようなタグはprereleaseとして扱う。
6. 下書きから配布ファイルを取得して専用環境・対象OSでGUIと再生を確認し、
   リリースノートを編集してGitHub上で公開する。自動公開はしない。

タグは`v`とCargo.tomlのバージョンが完全一致する必要がある。
いずれかのビルドや検査が失敗した場合、下書き作成へ進まない。
同じタグのActionsを再実行すると既存の下書きの添付ファイルを更新する。
公開済みリリースのファイルは上書きせず停止する。
artifactの保存期限を過ぎた場合は、ビルドを含む全ジョブを再実行する。

## GitHub側の設定と保守

通常のCIジョブは`contents: read`のみを設定し、リリースジョブにのみ`contents: write`を付与する。
標準の`GITHUB_TOKEN`でタグとリリースを作成・更新するため、追加のPATや配布用秘密鍵は不要である。
リポジトリー／OrganizationのポリシーでActionsと利用するActionが許可されている必要がある。
リポジトリやOrganizationの設定では、`latest-build`タグの更新を許可し、リリースの不変性（immutable releases）を無効にしておく必要がある。
固定リリースの差し替えにはタグの強制移動と添付ファイルの変更を伴うため、不変性が有効なリリースは更新できない。
既存の`latest-build`が通常リリース（非Pre-release）または不変リリース（immutable）であった場合は、書き込みを行わずに停止する。

`main`の保護ルールを設定する場合は、上記4つのCIジョブを必須チェックに指定する。
リリースジョブはpull requestで実行しないため、必須チェックには指定しない。
ワークフローの追加自体ではGitHub側の保護ルールは変更されない。

外部ActionはコミットSHAで固定し、[Dependabot](../.github/dependabot.yml)で毎週更新を提案する。
UbuntuベースイメージのdigestもDependabotの対象。Rust・Qt・GStreamer・actionlintの
バージョンとSHA-256は対応するDockerfile／スクリプト／ワークフローで明示的に更新する。
Flatpakの同一SDKブランチ内の更新とUbuntuのapt更新は可変のため、ビット単位の再現性は保証しない。

GitHub Actions公式の`concurrency.queue`（`queue: max`）に対して、actionlint 1.7.12は未対応である。
そのため、[actionlint設定](../.github/actionlint.yaml)でこのキーの未対応診断のみを除外している。
対応版へ更新した時点でこの除外設定を削除する。他の構文、式、シェルスクリプトの検査は継続する。

設定変更時の軽量な確認:

```sh
python3 scripts/test-release.py
python3 scripts/check-release-metadata.py
python3 scripts/flatpak-cargo-sources.py --check
bash -n scripts/ci.sh scripts/create-release.sh scripts/build-appimage.sh scripts/build-deb.sh scripts/test-deb.sh
actionlint
```

2026-09-21のローカル検証では、リリース処理のテスト（`python3 scripts/test-release.py`の24テスト）、ShellCheck、`bash -n`、
上記除外を適用したactionlint、`git diff --check`、メタデータ検証、Flatpak依存一覧チェックはローカルで確認済みである。
リリース処理テストは一時Gitリポジトリとローカルの疑似GitHub CLI応答を使い、
バージョン不一致、破損・欠落した成果物、API失敗時の挙動、タグとコミットの対応、
最新ビルドの差し替えと古い成果物の削除、失敗後の再実行、古いビルドの除外、
通常の公開済みリリースや不変リリースの保護をネットワーク通信なしで検証する。
この検証時点では、GitHub上でのCI実行や実リリースの更新は未確認であり、リモートへのpushは行っていない。

参照: [GitHub Actionsのartifact](https://github.com/actions/upload-artifact)、
[GitHub CLIのRelease作成オプション](https://cli.github.com/manual/gh_release_create)、
[GitHub CLIのRelease更新オプション](https://cli.github.com/manual/gh_release_edit)、
[GitHub Actionsの同時実行制御](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency)。
