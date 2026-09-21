#!/usr/bin/env python3
"""Generate Japanese download instructions for the rolling pre-release."""

import argparse
from urllib.parse import quote

from package_metadata import UbuntuRelease, release_assets


def release_notes(version: str, commit: str, repository: str) -> str:
    base_url = f"https://github.com/{repository}"
    source_url = f"{base_url}/blob/{commit}"
    download_url = f"{base_url}/releases/download/latest-build"
    packages = ["AppImage（glibc 2.39以降のLinux）", "Flatpak（Flatpak導入済みのLinux）"] + [
        f"Ubuntu {ubuntu.value}用deb" for ubuntu in UbuntuRelease
    ]
    rows = []
    for label, filename in zip(packages, release_assets(version), strict=True):
        url = f"{download_url}/{quote(filename, safe='')}"
        rows.append(f"| {label} | [{filename}]({url}) | [SHA-256]({url}.sha256) |")
    downloads = "\n".join(rows)
    return f"""## ダウンロード

**下の表から、お使いの環境に合うパッケージを1つダウンロードしてください。** 配布対象はLinux x86_64（amd64）です。

| 形式・対象環境 | ダウンロード | チェックサム |
| --- | --- | --- |
{downloads}

ページ下部の **Assets** からも取得できます。ファイル一覧が見えない場合は、Assetsの見出しをクリックして開いてください。
`Source code (zip)`・`Source code (tar.gz)`は開発者向けのソースコードで、インストール用パッケージは上の表にあるファイルです。

## インストールと使い方

ながめTVは、Mirakurun経由の地デジ・BS・CS視聴と、ローカルのTS録画ファイル再生に対応したLinux向けアプリです。
番組表、ARIB字幕、NX-Jikkyoの実況コメント表示・投稿を利用できます。

- Ubuntuでは、お使いのOSバージョンに対応するdebを選んでください。
- FlatpakはFlatpak本体の導入が必要です。必要なランタイムはインストール時にFlathubから取得します。
- AppImageはダウンロード後に実行権限を付けて起動します。
- 画面表示にはX11またはXWaylandとOpenGL対応GPU、音声出力にはPipeWireまたはPulseAudioが必要です。
- ライブ視聴と番組表には、別途稼働中のMirakurunサーバーが必要です。TS録画ファイルだけならサーバー未設定でも再生できます。

コマンドと初回設定は[READMEのインストール手順]({source_url}/README.md#インストール)を参照してください。
[AppImageの互換要件]({source_url}/docs/appimage.md#起動する)・[Flatpakの導入手順]({source_url}/docs/flatpak.md#インストール)も確認できます。

SHA-256を確認する場合は、パッケージと対応する`.sha256`を同じフォルダーに保存し、`sha256sum --check ダウンロードしたファイル名.sha256`を実行してください。

## この開発ビルドについて

`main`のCI（自動テスト・配布ビルド）成功後に更新するプレリリースです。正式リリース前の変更を含みます。
同じバージョン・ファイル名のまま内容が更新されるため、不具合報告には下記のコミットを添えてください。
更新版はこのページから再ダウンロードしてください。

- バージョン: `{version}`
- コミット: [`{commit}`]({base_url}/commit/{commit})
- [不具合の報告]({base_url}/issues)
"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("version")
    parser.add_argument("commit")
    parser.add_argument("repository")
    args = parser.parse_args()
    print(release_notes(args.version, args.commit, args.repository))


if __name__ == "__main__":
    main()
