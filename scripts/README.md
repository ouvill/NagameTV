# 開発用スクリプト

コマンドはリポジトリーのルートで実行します。アプリの通常ビルドにはCMake、
テストには `python3 scripts/test.py` を使います。Pythonは3.11以降が必要です。
個別のテスト用シェルスクリプトは共通ランナーへ統合しました。

## 配置と依存関係

| 配置 | 役割 |
| --- | --- |
| `scripts/test.py` | テスト選択、ビルド順序、実行結果の記録 |
| `scripts/build/` | ビルドロック、ソース情報、CIのローカルクレート再ビルド |
| `scripts/testing/` | スイート定義、Qtバイナリー準備、専用GUI環境、静的検査、結合試験 |
| `scripts/packaging/` | AppImage・deb・Flatpakの作成、配布物検証、リリース |
| `scripts/dev/` | 手動起動、計測、診断、画像撮影、文書生成、環境セットアップ |
| `tests/tooling/` | スクリプト自身の機器不要の回帰テスト |
| `tests/fixtures/` | テストデータと `generate_*.py` による再生成 |

通常のCMakeビルドが使うスクリプトは `build/with-build-lock.sh` と
`build/source_info.py` です。Flatpakにもこの2本を同じ配置で渡します。
`build/` は `testing/` や `dev/` に依存させません。
配布設定とDockerfileは従来どおりリポジトリー直下の `packaging/` に置きます。

web-bml のブラウザー用 JS を更新するときは `python3 -m scripts.build.update_web_bml` を実行します。
Git、Node.js、npm とネットワークが必要で、固定コミットから `assets/web-bml/bundle.js` と
ライセンスを生成します。通常のビルドや実行時には Node.js を使いません。

## ビルドとテスト

```sh
cmake -S . -B build -DCMAKE_BUILD_TYPE=Debug -DNAGAMETV_DISTRIBUTION=OFF
cmake --build build
python3 scripts/test.py --list
python3 scripts/test.py                          # 機器不要の標準テスト
python3 scripts/test.py core                     # Qt/GStreamer不要のRustクレートをまとめて検証
python3 scripts/test.py tooling                  # スクリプト自身の回帰テスト
python3 scripts/test.py arib-b24                 # データカルーセルの機器不要テスト
python3 scripts/test.py viewer-web-bml            # web-bml への変換の機器不要テスト
python3 scripts/test.py web-bml-adapter           # ブラウザー接続・起動制御。開発用Node.jsが必要
python3 scripts/test.py viewer-mpegts            # 共通TS・PAT/PMTの機器不要テスト
python3 scripts/test.py libaribcaption           # 字幕デコーダーの所有権試験。C++/CMake/libclangが必要
python3 scripts/test.py connection localization  # 機器不要のQt試験
python3 scripts/test.py danmaku ui-style          # 専用画面・実GPU・仮想音声を検証
python3 scripts/test.py startup -- recording-pid-change
python3 scripts/test.py danmaku -- --evaluation-legacy-comments
```

スイート固有の引数は、スイートを1つ選んで `--` の後に渡します。
`startup -- recording-audit TS_PATH`、`startup -- recording-probe TS_PATH`、
QML試験の関数指定や `-o` も同じ形式です。`checks` には `tooling` と翻訳カタログ検査を
含み、同時に選んでも各検査は一度だけ実行します。
標準実行ではQt不要のRustクレートを先に検証します。クレートごとにビルド・実行するため、
テスト失敗後に残りのクレートやQtをビルドしません。`core`はアプリ・Qtの検証を含まないので、
変更範囲に応じて選択します。CIと引数なしの実行は引き続き全CPUスイートを検証します。

GUI試験は [gui_session.py](testing/gui_session.py) が専用画面と音声出力を所有し、
検証成功後に [suites.py](testing/suites.py) がテストを実行します。
機器不足や接続先の変更は失敗として扱います。旧字幕試験の `--ui-only` は廃止し、
描画試験をこの専用環境に統一しました。物理機器を使わない字幕形状の検査には
`python3 scripts/test.py subtitle-outline` を使います。

共通ランナー、CMake、直接実行するCargo診断はビルドロックを共有します。

```sh
bash scripts/build/with-build-lock.sh cargo clippy --manifest-path rust/Cargo.toml --locked --all-targets -- -D warnings
```

ログと結果は `build/test-runs/run-*/`、GUI環境のログは `build/gui-tests/` に保存します。
詳細は [開発手順](../docs/development.md)、[Qtテスト](../docs/qt-tests.md)、
[GUI環境](../docs/gui-test-environment.md)を参照してください。

## 配布と個別の開発作業

| 用途 | 公開コマンド | 要件・説明 |
| --- | --- | --- |
| AppImage | `bash scripts/packaging/build-appimage.sh` | Docker。[手順](../docs/appimage.md) |
| deb | `bash scripts/packaging/build-deb.sh 24.04` または `26.04` | Docker。[手順](../docs/deb.md) |
| Flatpak | `bash scripts/packaging/build-flatpak.sh` | Flatpak SDK。[手順](../docs/flatpak.md) |
| リリース | `bash scripts/packaging/create-release.sh TAG ASSET_DIRECTORY` | GitHubを書き換える。[手順](../docs/ci-release.md) |
| 固定版EPGStationとの結合試験 | `python3 -m scripts.testing.epgstation_integration` | Docker、機器不要。[手順](../docs/epgstation.md) |
| コメントSQL検査・更新 | `python3 -m scripts.testing.check_comment_sql [--update]` | Cargo、機器不要。一時DBを使用 |
| Flatpakソース一覧の更新 | `python3 -m scripts.packaging.flatpak_cargo_sources [--check]` | Cargo.lockと配布設定の整合性 |
| アプリの手動起動 | `bash scripts/dev/run-viewer.sh` | ビルド済みアプリと表示・GPU・音声環境 |
| heaptrack計測 | `bash scripts/dev/profile-memory.sh` | 手動の表示・GPU・音声環境。[手順](../docs/memory-profiling.md) |
| 診断ログの解析 | `python3 -m scripts.dev.analyze_memory --output DIRECTORY` | 機器不要。既存ログからHTMLを生成 |
| 外部PIDのメモリー監視 | `bash scripts/dev/monitor-memory.sh PID [INTERVAL [CSV]]` | Linuxの `/proc` を読み、RSS・PSS・スレッド数・FD数をCSVへ記録。アプリ内診断のないプロセスにも使用可能 |
| 再生・字幕のメモリー計測 | `python3 -m scripts.dev.benchmark_playback_memory --help` / `python3 -m scripts.dev.benchmark_subtitle_memory --help` | 実行時は専用GUI環境。[再生](../docs/playback-memory-architecture.md)・[字幕](../docs/subtitle-memory.md) |
| UI部品の画像生成 | `python3 scripts/test.py ui-capture` | 専用GUI環境。`build/ui-review/` に出力 |
| 公開用スクリーンショット | `bash scripts/dev/capture-publicity.sh` | 専用GUI環境。[準備](../docs/publicity.md) |
| IME診断 | `bash scripts/dev/diagnose-ime.sh wayland` または `ibus` | 問題が起きるデスクトップで手動実行。[手順](../docs/platform-startup.md) |
| API文書生成 | `bash scripts/dev/generate-remote-docs.sh` | bufとprotoc-gen-doc。[手順](../docs/remote-control.md) |

Fedoraの導入は `bash scripts/dev/setup-fedora.sh`、nextestの導入は
`bash scripts/testing/install-nextest.sh` です。テストデータの生成は
[fixturesの説明](../tests/fixtures/README.md)を参照してください。

## 追加・変更するとき

- 同じスイートの引数・入力・環境だけが違う場合は、既存の定義を拡張します。
  公開入口やGUIの資源検証を複製しません。
- 引数処理、条件分岐、データ加工、プロセス管理はPythonを基本とします。
  ShellはOSコマンドの接続や短い起動処理に使います。Pythonモジュール名は `snake_case` とし、
  原則として `python3 -m scripts.<用途>.<モジュール>` で起動します。
- 継続して使うスクリプトには、用途・呼び出し元・必要資源・出力先をここか対応する文書に記載します。
  一度限りの調査コードはGit対象外の作業ディレクトリーに置き、再利用する道具や回帰テストに
  なった段階で取り込みます。
- 移動時はCI、Workshop、CMake、Flatpakのソース一覧、文書を更新します。
  ファイル名の直接参照がなくても、自動収集されるテストや手動ツールは削除候補から区別します。
