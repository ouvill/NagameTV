# 言語切り替えの移植

mainのlocalization.hとtranslations/app_ja.ts（108項目）を移植し、同じ/i18n/ja.qmを
ビルド時に生成・同梱する。現段階ではアプリの言語設定・QML各文言へまだ接続していない。
日本語が直接書かれている現在のQMLへ単にカタログを入れるだけでは英語にならない。
mainの英語原文と翻訳コンテキストを照合し、各文言を翻訳可能なバインディングにする作業が残る。

## Qt側の寿命と再翻訳

mainと同じく英語を原文にし、日本語のQTranslatorをQCoreApplicationが1個所有する。
日本語を最初に要求した際に読み込み、切り替え後も再利用する。QPointerはエンジン・
翻訳器の破棄を観測する。日本語以外のシステム言語は英語へ解決する。
日本語カタログを読み込めない場合は空の結果を返し、有効言語を変更しない。
Rust側はこの結果を確認してから設定を保存する必要がある。

[QQmlEngine::retranslate](https://doc.qt.io/qt-6/qqmlengine.html#retranslate)で翻訳用の
バインディングを更新し、QMLオブジェクトを作り直さない。
[QTranslator](https://doc.qt.io/qt-6/qtranslator.html)の寿命はアプリが管理し、
言語を切り替えるたびに翻訳器を増やさない。更新はGUIスレッドから行う契約。

## ビルド

build/translations.rsを分離し、QT_LRELEASEの指定またはqtpaths6/qtpathsの
QT_HOST_BINSからlreleaseを取得する。カタログと生成qrcはCargoのOUT_DIRへ置き、
生成ファイルをソースディレクトリーへ書き戻さない。
ツール不在・カタログ生成失敗・出力失敗はResultでビルドを失敗させる。
TSファイルとQT_LRELEASE変更をCargoの再ビルド条件へ登録する。

## 検証

scripts/test-localization.shはQCoreApplicationとQtObjectだけで動く専用試験。
ディスプレイ・GPU・音声も、代替の描画バックエンドも使わない。
mainのロケール解決、既存QML文言の日本語・英語への再翻訳、Backendコンテキストの翻訳、
100往復後の翻訳器1個保持を確認した。カタログをリンクしない別バイナリーでは、
読み込み失敗・英語状態の保持・失敗した翻訳器の解放を確認した。両方成功。
テスト用ファイルは一時ディレクトリーへ生成し、終了時に削除する。

CMakeリリースビルドも成功。実アプリでの切り替え、言語設定の永続化、全画面の翻訳対象と
英語表示時の寸法・折り返しの照合は未実施。実再生を伴うアプリは起動していない。
