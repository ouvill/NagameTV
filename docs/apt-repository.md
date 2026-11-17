# aptリポジトリー

Ubuntu 24.04／26.04向けdebを`apt upgrade`で更新できるように、署名したaptリポジトリーを
GitHub Releasesで提供する。

**状態: 準備中。** 公開処理は既定OFFのリポジトリー変数`APT_REPOSITORY_ENABLED`で止めている。
署名鍵は2026-11-14に作成し、公開鍵をdebへ同梱した。実URLでの初回公開の確認が済むまで、
[debの導入手順](deb.md)では案内しない。

| 鍵 | フィンガープリント |
| --- | --- |
| 親鍵（証明専用、期限なし） | `1EF9 5CA3 649A 2F1A C857  C0C4 1CAF F38F E9C2 88A2` |
| 署名用副鍵（期限なし） | `A1F6 E537 7E5B C222 274B  26C4 58D1 C4EA 25B2 A4B2` |

## 構成

| 項目 | 内容 |
| --- | --- |
| 索引 | GitHub Release `apt-noble`（24.04）・`apt-resolute`（26.04）に`Packages`・`Release`・`InRelease`だけを添付する |
| debの取得先 | `Packages`の`Filename`は`v0.8.0/nagametv_0.8.0-1ubuntu24.04_amd64.deb`のように、各バージョンのリリースの添付ファイルを指す。debを重複してアップロードしない |
| aptの設定 | flat形式。`URIs`はリリースのダウンロード起点、`Suites`は`apt-noble/` |
| CPU | 24.04はamd64、26.04はamd64・arm64。同じ`Packages`へCPU別の項目を載せ、`Release`の`Architectures`にも列挙する |
| 対象 | 公開済みの正式版のみ。下書き・rcなどのPre-releaseは載せない |
| 署名 | 署名専用の副鍵でInReleaseに署名する。親鍵はオフラインで保管する |

aptは`https://github.com/ouvill/NagameTV/releases/download/apt-noble/InRelease`を取得し、
GitHubはファイル配信用のホストへリダイレクトする。`Release`の`Suite`は利用者が設定する
`Suites`と一致させる。一致しないとaptが`Conflicting distribution`の警告を出す。

`apt-*`のリリースは`--latest=false`で作成し、GitHubのLatestにしない。
AppImageの更新情報と`/releases/latest`は通常のバージョンリリースを指したままになる。

`Release`には`Valid-Until`を付けない。リリースは不定期のため、期限を付けると
更新がない期間に利用者の`apt update`が失敗する。

索引は`Packages`、`Release`、`InRelease`の順に1つずつ置き換える。
置き換え中の数秒間に`apt update`するとハッシュの不一致で失敗することがあるが、再実行で解消する。

公開処理は[`publish_apt_repository.py`](../scripts/packaging/publish_apt_repository.py)が行う。
GitHubへ書き込む前に、次の条件をすべて確認し、両方のUbuntu向けの索引に署名を終える。

- 指定したタグが公開済みの正式版リリースであること
- ダウンロードしたdebが、GitHubに記録されたリリース添付ファイルのSHA-256と一致すること
- 対象Ubuntuの全CPU向けdebが揃い、パッケージ名・アーキテクチャー・バージョンがタグに合うこと
- 公開中の版より古い版で上書きしないこと（同じ版の再実行は可）
- 署名がリポジトリーの公開鍵[`nagametv-archive-keyring.gpg`](../packaging/deb/)で検証できること

## 署名鍵の方針

- 親鍵は証明専用（cert）で有効期限なし。オフラインで保管する。
- 署名用副鍵（sign）も有効期限なし。GitHub ActionsのEnvironment secretに副鍵だけを置く。
- 公開鍵は`packaging/deb/nagametv-archive-keyring.gpg`に置き、debに
  `/usr/share/keyrings/nagametv-archive-keyring.gpg`として同梱する。鍵の追加・失効を
  通常の`apt upgrade`で利用者へ届けるためである。
- 利用者の設定では`Signed-By`でこの鍵の適用範囲をこのリポジトリーだけに限定する。

有効期限は付けない。期限が切れると全利用者の`apt update`が署名エラーになり、
新しい鍵もaptでは届けられず、手動で入れ直してもらう必要がある。
2026-11時点で、Ubuntu本体、Docker、Microsoft、PostgreSQL、Mozilla、Signal、
GitHub CLI（2026年に作成した新しい鍵）などの主要な外部リポジトリーも期限なしの鍵を使っている。
期限を付けた場合の利点（漏えいに気付かない場合や鍵を失った場合に自動で無効になる）は、
親鍵と失効証明書をオフラインで保管することで代える。

## 署名鍵の作成と登録

保守者が一度だけ行う。WorkshopのコンテナーやCIではなく、保守者自身のPCで実行する。
親鍵にはパスフレーズを設定する。

```sh
export GNUPGHOME="$(mktemp -d)"
gpg --quick-gen-key 'NagameTV apt repository' ed25519 cert never
primary=$(gpg --list-keys --with-colons | awk -F: '$1=="fpr"{print $10; exit}')
gpg --quick-add-key "$primary" ed25519 sign never
subkey=$(gpg --list-keys --with-colons "$primary" | awk -F: '$1=="fpr"{print $10}' | sed -n 2p)
echo "primary=$primary subkey=$subkey"

# リポジトリーへコミットする公開鍵
gpg --export "$primary" > nagametv-archive-keyring.gpg
# オフライン保管: 親鍵を含む秘密鍵と、作成時に生成される失効証明書
gpg --armor --export-secret-keys "$primary" > nagametv-apt-primary-secret.asc
cp "$GNUPGHOME/openpgp-revocs.d/$primary.rev" nagametv-apt-revocation.rev
# Actions用: 署名用副鍵だけを書き出す
gpg --armor --export-secret-subkeys "$subkey!" > signing-subkey.asc
```

Actionsでは対話入力できないため、書き出した副鍵だけを別の作業用ディレクトリーへ取り込み、
そこでパスフレーズを外す。親鍵の秘密部分がないため、親鍵について
`error changing passphrase: No secret key`と表示されるが、副鍵は変更される。

```sh
export GNUPGHOME="$(mktemp -d)"
gpg --import signing-subkey.asc
gpg --passwd "$subkey"   # 現在のパスフレーズを入力し、新しいパスフレーズを空にする
gpg --armor --export-secret-subkeys "$subkey!" > signing-subkey-nopass.asc
# パスフレーズなしで署名できることを確認する
echo test | gpg --pinentry-mode error --local-user "$subkey!" --clearsign > /dev/null && echo ok
```

GitHubで次を設定する。

1. Settings → Environments で`apt-repository`を作成する。
   - Required reviewers: 保守者自身。公開のたびに承認してから署名する。
   - Deployment branches and tags: Selected。タグ`v*`とブランチ`main`だけを許可する。
2. Environment secret `APT_SIGNING_KEY`に`signing-subkey-nopass.asc`の内容を登録する。
3. リポジトリー変数`APT_SIGNING_KEY_FINGERPRINT`に副鍵のフィンガープリントを登録する。

```sh
gh secret set APT_SIGNING_KEY --env apt-repository < signing-subkey-nopass.asc
gh variable set APT_SIGNING_KEY_FINGERPRINT --body "$subkey"
```

登録後、`signing-subkey.asc`、`signing-subkey-nopass.asc`、作業用の`GNUPGHOME`を削除する。
`nagametv-apt-primary-secret.asc`と`nagametv-apt-revocation.rev`は
オフラインの媒体2か所に保管する。親鍵のパスフレーズも別に保管する。

## 有効化

1. `nagametv-archive-keyring.gpg`を`packaging/deb/`に追加し、debへの同梱とともに`main`へ取り込む
   （同梱は[`build-deb.sh`](../scripts/packaging/build-deb.sh)、導入後の一致確認は
   [`check_installed_deb.py`](../scripts/packaging/check_installed_deb.py)）。
2. リポジトリー変数`APT_REPOSITORY_ENABLED`を`true`にする。
3. 公開鍵を同梱した版のリリースを公開する。または既存の版で手動実行する。

   ```sh
   gh workflow run apt-repository.yml -f tag=v0.9.0
   ```

4. Environmentの承認待ちを承認する。
5. クリーンなUbuntu 24.04／26.04コンテナーで[利用者向けの手順](#利用者向けの導入手順)を実行し、
   実URLから導入・更新できることを確認してから[debの導入手順](deb.md)に記載する。

## 公開の流れ

リリースの下書きを公開すると、[aptリポジトリーのワークフロー](../.github/workflows/apt-repository.yml)が
起動し、Environmentの承認を待つ。承認後、公開済みのdebを取得し、索引に署名して`apt-*`へ添付する。
Pre-releaseと`v`で始まらないタグは対象外。
既存の版の再公開や失敗時の再実行は、上記の`gh workflow run`で行う。

## 利用者向けの導入手順

有効化後に[debの導入手順](deb.md)へ移す。24.04では`Suites`を`apt-noble/`、
26.04では`apt-resolute/`にする。ARMは26.04用のみを対象とする。

```sh
curl -fsSL https://raw.githubusercontent.com/ouvill/NagameTV/main/packaging/deb/nagametv-archive-keyring.gpg \
  | sudo tee /usr/share/keyrings/nagametv-archive-keyring.gpg > /dev/null
gpg --show-keys /usr/share/keyrings/nagametv-archive-keyring.gpg   # 上表のフィンガープリントと比べる
sudo tee /etc/apt/sources.list.d/nagametv.sources > /dev/null <<'EOF'
Types: deb
URIs: https://github.com/ouvill/NagameTV/releases/download/
Suites: apt-noble/
Signed-By: /usr/share/keyrings/nagametv-archive-keyring.gpg
EOF
sudo apt update
sudo apt install nagametv
```

鍵ファイルはdebも同じパスに同梱するため、導入後はパッケージの更新で置き換わる。
`apt remove nagametv`では鍵ファイルも削除されるので、`nagametv.sources`も削除する。

Ubuntuを24.04から26.04へ更新すると、外部リポジトリーは無効化される。
`nagametv.sources`の`Suites`を`apt-resolute/`に変更し、無効化の設定を戻してから
`sudo apt update && sudo apt upgrade`を実行する。

## 鍵の追加と失効

aptは鍵サーバーに問い合わせないため、鍵の変更はdebに同梱した公開鍵の更新で届ける。
利用者の手元にない副鍵で署名すると`apt update`が失敗するので、次の順に行う。

1. オフラインの親鍵で新しい署名用副鍵を追加し、両方の副鍵を含む公開鍵をdebに同梱して
   旧副鍵で署名したリリースを公開する。
2. 利用者が更新できる期間を置いてから、Actionsの秘密鍵とフィンガープリントを新しい副鍵へ
   替える。旧副鍵を失効させた公開鍵を同梱する。

旧副鍵の漏えいが確実な場合は、すぐに失効させて新しい副鍵へ替える。
その場合、更新していない利用者は鍵を手動で入れ直す必要があるため、リリースノートと
READMEで案内する。

## 検証

```sh
python3 scripts/test.py tooling
```

使い捨ての鍵とダミーのdebで、索引の内容、署名の検証、リポジトリー外の鍵での署名の拒否、
GitHubへの書き込み前の停止条件、作成・更新の順序を確認する。表示・GPU・音声は使わない。

2026-11-14に、Ubuntu 24.04コンテナーとGitHubのリダイレクトを模したローカルHTTPサーバーで、
この構成の`apt update`、`apt install`、0.8.0から0.8.1への`apt upgrade`を確認した。
実際のGitHub Releasesからの導入・更新は、[有効化](#有効化)の手順5で確認する。
