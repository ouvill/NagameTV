# 言語切り替えの移植

mainのlocalization.hとtranslations/app_ja.ts（108項目）を移植し、同じ/i18n/ja.qmを
ビルド時に生成・同梱する。言語設定とカタログ対応済みの文言へ接続した。
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


## 設定・アプリへの接続

Language enumはSystem / Japanese / Englishを持ち、保存形式はmainと同じsystem / ja / en。
設定に言語がなければSystem、不明な永続文字列はmain同様にEnglishへ正規化する。
UIからの不明なコードは受け付けない。既存languageをextraから型付きフィールドへ移行した。

mainと同様、エンジンへQMLをロードする前に保存設定の言語を読み、翻訳を初期化する。
Playerの完全な設定セッション読込は既存経路を維持するため、起動時の設定読み込みは2回。
--featuresによる隔離起動は保存設定を使わずSystemで開始する。
初期翻訳の失敗は起動エラーとして扱い、選択した言語を黙って別のものへ置き換えない。
変更要求はQt側applyUiLanguageの成功後だけPreferences・公開プロパティを更新して保存する。
設定画面はmainの言語ComboBoxの配色・サイズを移し、失敗時はエラー表示と選択の復元を行う。

元の大きなMain.qmlから各コンポーネントへ分割したため、翻訳コンテキストを
qsTranslate("Main", ...)で明示して元のカタログを共有する。カタログの日本語と一致した
54か所に加え、音声の番号付き表示と音声選択タイトル等を既存原文へ対応させた。
本文・番組名などサーバーからのデータを翻訳キーへ変換することはしない。

設定の互換性・永続化・未知のコードの扱いを含むCPUテスト7件が成功。
QtCore/QML試験にはqsTranslateの共有コンテキストの再翻訳も追加し、正常系と
カタログ欠落系が成功。SettingsDrawer / ProgramSidebar / StoppedPlayback /
AudioSettingsのqmllintと全ターゲットClippyが成功。
音声表示のQML試験は期待値を翻訳対応にしたが、GUIを必要とする試験は未実行。

カタログに一致しないQMLの日本語、音声の補足説明、EPGの日付・ジャンル・操作案内、
バックエンドの状態・エラー文の翻訳が残る。現時点の英語表示は一部に日本語が残る。
英語時の寸法・折り返しと切り替え操作そのものの実画面検証も未実施。

接続後のCMakeリリースビルドとfmtも成功。


## 固定文言の補完

追加47項目をViewerコンテキストへ登録し、固定文言61か所を翻訳対応にした。
mainから引き継いだ108項目と合わせて155項目。新実装固有の文言は元の日本語を維持する。
チャンネルの空表示、番組情報の未取得表示、再生失敗案内、実況設定、ウィンドウ操作、
動画統計の見出し・注記が対象。動的な番組名や説明はそのまま表示する。

本体QMLの直接書かれた日本語を再検索し、残りは日本語・中文という言語の自称表記だけ。
ただしRustから届く状態文・ジャンル・エラーはこの検索に含まれず、全UIの英語化完了を
意味しない。日付表示のロケール連動と英語時の実画面レイアウトも未検証。

QMLの固定qsTranslateキー81種類をカタログへ照合し、欠落なし。
QtCore/QML試験で追加Viewerコンテキストの日本語・英語への再翻訳も成功。
既存の翻訳器100往復・カタログ欠落試験、変更した18コンポーネントのqmllint、
CMakeリリースビルドが成功。動画統計のdelegate参照はidで明示し、静的解析警告も解消した。
番組名未取得表示のQMLテスト期待値を翻訳対応にしたが、GUI試験そのものは未実行。


## 番組表の日付ロケール

Player.ui_languageをProgramGuide → GuideToolbar → GuideDateSelectorへ渡し、
曜日のロケールを選択言語に連動させた。単独利用時はQt.uiLanguageを使用する。
mainのMainコンテキストの日付書式を共有し、英語はddd, MMM d、日本語はM/d（ddd）。
通常表示とコンパクト表示は同じlabel関数を使う。
Qtの[Date.toLocaleDateString](https://doc.qt.io/qt-6/qml-qtqml-date.html#string-date-tolocaledatestring-locale-format)
へロケールと書式を明示する。

calendarDaysの未使用labelを削除し、取得範囲にはstart/endのみ保持する。
言語変更はdays/selectedWindowの依存関係に入らず、EPG取得要求や選択の解除を起こさない。
既存のローカル日付による日境界と固定24時間表記の番組時刻は維持する。

QtCore/QML試験で既定ロケールをde_DEにし、英語Tue, Sep 8 → 日本語9/8（火）→
英語への再翻訳と元の日付値の保持を確認。これはQtの日付・翻訳連動の試験であり、
GuideDateSelector自体のGUI試験ではない。カタログ欠落試験、対象3コンポーネントの
qmllint、CMakeリリースビルドも成功。英語の日付ラベルの実画面での収まりは未検証。


## 実況・EPGの状態表示

実況のPresentationStatusはDisabled / Unavailable / Connecting / Receiving /
Failed / Retryingを持ち、接続失敗は借用する。機能層では表示文字列を確保しない。
EPGは既存のStatusを使用し、player/status.rsでQtのBackend翻訳へ変換する。
カタログへ9項目を追加し、mainの実況状態の既存キーも共有する。

[QCoreApplication::translate](https://doc.qt.io/qt-6/qcoreapplication.html#translate)
でテンプレートを翻訳し、QString::argで件数・診断詳細を挿入する。
診断詳細は翻訳キーとして検索せず、その中の%1やマークアップもそのままデータとして扱う。
既存のQML表示先はText.PlainText。起動時のDisabledも翻訳する。
言語変更時は既存状態の表示を直接更新し、poll/configure/再接続/EPG取得を呼ばない。
通常ポーリング時の投影は従来通りで、翻訳結果の履歴や独自キャッシュは保持しない。

今回の範囲は状態と失敗の見出し。thiserrorによる低層の診断詳細、再生状態・エラー、
字幕状態、ジャンルの日本語は残る。失敗詳細まで英語化されたことは意味しない。
また既存のJSON表示失敗は状態とは別の一時的表示で、通常ポーリングで上書きされる
既存挙動を維持している。これらも含めた全バックエンド翻訳は未完了。

機能層のCPU試験47件が成功、TS実ファイルが必要な既存試験1件はignored。
実況の無効・未対応・接続開始・解除時の型付き状態も既存の履歴寿命試験で確認した。
QtCore/QML試験では件数の位置の翻訳、再接続状態の日英切り替え、診断詳細に含まれる
%1等の保持を確認。カタログ欠落試験と全ターゲットClippy（警告をエラー扱い）も成功。
状態表示の翻訳キー12種類をカタログと照合し欠落なし。実画面での表示と、
実接続中に言語を切り替えた場合の操作確認は未実施。

CMakeリリースビルドも成功。


## 再生失敗の案内とジャンル記述の訂正

mainのplayback.rsを参照し、HTTP 503 / 404 / 401・403 / 408・504 /
その他5xx / その他4xxに応じた案内を移植した。HTTP情報がなければ、souphttpsrcの
ResourceErrorだけを通信失敗として扱い、出力装置のエラーを通信障害と誤表示しない。
Errorを文字列化する前にHint enumへ分類し、診断用のnative errorとdebug情報は保持する。
Cleanupはprimaryの分類を引き継ぐ。同期的なPLAYING遷移失敗でも、停止がbusを破棄する前に
既に届いた具体的なエラーを取り出す。

[GStreamerの公式API](https://gstreamer.freedesktop.org/documentation/gstreamer/gstmessage.html#gst_message_parse_error_details)
でdetailsは省略可能な借用構造であることを確認。Rust bindingでhttp-status-codeをu32として
取り出し、欠落・型不一致はNoneにする。エラー文の数字をHTTPコードと推測しない。

QMLへは固定の翻訳原文を一つ渡し、qsTranslateのBackendコンテキストで案内を表示する。
mainの既存9キーを共有し、言語変更時に表示中の案内も再翻訳される。
再生再試行・接続先変更・PLAYING到達で案内と診断詳細をまとめて解除する。
既存の詳細表示・ログ保存には技術的な診断文を使い、これらの全面翻訳は残る。

以前の残作業に「ジャンルの翻訳」と書いたが、main・実験版のQMLとEPG投影を再確認すると、
ジャンル値は番組表の色分けにだけ使われており、翻訳対象のジャンルラベルはない。
これは未移植機能ではなかったため訂正する。言語変更によるEPG JSON再生成も不要。

生成GStreamerメッセージを使うCPU試験2件で12種類のHTTPコード、details欠落・型不一致、
ネットワーク発生元の区別、元の失敗をCleanup後も保持することを確認。
既存のHTTPソースのseek失敗だけを自動再接続対象とする試験も成功した。
QtCore/QML試験で動的な翻訳原文による表示中エラーの再翻訳、qmllint、全ターゲットClippyも成功。
実サーバーの503等を引き起こす試験、同期遷移失敗の実再生試験、実画面の配置確認は未実施。

CMakeリリースビルドも成功。
