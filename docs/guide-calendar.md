# 番組表の日付選択と予定番組の詳細

mainの番組表の日付選択（今日から7日間）と予定番組の詳細表示を移植する。
今回は選択局の一覧を拡張し、複数局を並べる時間軸は引き続き未移植。

## 日付と範囲

QMLはローカル暦の午前0時から翌午前0時を計算する。
[ECMAScript Date.setDate](https://tc39.es/ecma262/multipage/numbers-and-dates.html#sec-date.prototype.setdate)
に基づき日付を加算し、常に86,400,000msを足す方式にはしない。夏時間の23時間／25時間の日も含む。
開いている番組表だけが1分ごとに日付を確認し、ローカル日付が変わった場合だけ7日分を作り直す。
起動時の日付を固定して翌日以降に古い「今日」を表示し続けない。

Rust境界では `DayWindow` 型で有限・整数・非負・JavaScript Dateの範囲内・開始＜終了を検証する。
最大26時間の範囲だけを受け入れる。`Guide` enumはClosed／AwaitingDay／Showing(DayWindow)。
閉じた番組表は日付要求を受け入れない。QMLのLoaderを生成する前にAwaitingDayへ遷移させる。

選択日の範囲と交差する番組を表示する。前日から続く番組を含み、開始が翌日午前0時の番組と
当日午前0時に終了する番組は含まない。ゼロ時間は表示しない。
短い番組の多い日も欠落させないよう、旧200件制限を廃止した。
EPG取得側の32MiB・50,000番組という全体上限は維持する。

## 更新とメモリー

局のスケジュールはスナップショットから借用し、選択した日だけJSONへ投影する。
開いた時、選択日・選択局・EPG revisionが変わった時だけ生成する。
固定日の表示なので、従来の30秒ごとの同じJSONの再生成は行わない。
日付変更では追加HTTP要求を発生させず、取得済みデータを使う。
選択日の全テキストはUIモデルとして保持するが、7日分・全局分をQtへ複製しない。

[Qt ListView](https://doc.qt.io/qt-6/qml-qtquick-listview.html)で必要な範囲のdelegateを生成する。
カードをItemDelegateにしてクリックとキーボードの標準操作を利用する。
詳細はdelegateの中に保存せず、番組表が選択した番組オブジェクトを一つ保持する。
スクロールによるdelegate破棄に影響されず、説明を省略せずに読める。
詳細を閉じる、日付・局を変える、番組表そのものを閉じると詳細を破棄する。
開いた予定番組の詳細はその選択時点のデータ。EPG再取得で過去の読書内容を自動置換しない。

## 検証

Rust: 日付をまたぐ番組、終了境界、251件の表示、不正数値・範囲、23/24/25時間を検証する。
既存の取得更新・キャンセル・現在番組の試験も実行する。
Qt: 7日分と日付操作の要求、ローカル午前0時の連続性、300件でのdelegate限定生成、
スクロール後の詳細保持、日付変更時の詳細破棄を検証する。
日付試験は通常タイムゾーンとTZ=America/New_Yorkの両方で実行し、後者では2026年の
夏時間開始・終了日が23時間・25時間になることを明示的に確認する。
Qt試験前にDISPLAY・GPUを検出し、xdpyinfo・glxinfoで検証する。

複数局の時間軸、同時放送サブ局の整理、全機能併用での長時間メモリー測定は未完了。

2026-09-07: Rust 48件成功（外部TS依存1件は未実行）、Clippy全ターゲット警告なし、
CMakeビルド成功、ProgramGuideのqmllint警告なし。
Qt操作試験8ケース成功（初期化・終了を含む14件）。ニューヨークの追加実行は番組表3ケース成功。
実アプリで番組表ボタンを操作し、09/07の選択と当日の予定番組が表示されることを画像で確認した。
QMLエラーなし、正常終了。実アプリでの日付の反復切替・選局との併用は今後の検証対象。
証跡はGit対象外の `benchmark/guide-calendar/smoke.py`、`smoke.log`、`playing.png`。


## 複数局の時間軸表示への移行

単一局の縦リストから、mainの番組表本体に合わせた全画面・複数列の時間軸表示へ移行する。
GuideTimeline.qmlに222pxの局幅、88pxの固定局見出し、104pxの時刻欄、1分2.4pxの時間軸、
13pxの番組名、時間罫線・現在時刻線を分離した。日またぎ番組は表示日との重なりで配置する。

RustのSnapshot::grid_viewは既存Snapshotから選択日の番組を借用し、局indexごとのJSONを
生成する。EPGの二重保持や追加取得は行わない。networkId/serviceIdで照合し、配信用IDを
復元計算しない。対象日の全局分JSONはQtに渡すため、表示局だけの転送にはまだ絞っていない。

Qtでは表示範囲と左右1列のLoaderだけを有効にし、離れた局の番組要素とロゴを解放する。
[Loader.active](https://doc.qt.io/qt-6/qml-qtquick-loader.html#active-prop)の要素解放と
[Flickable](https://doc.qt.io/qt-6/qml-qtquick-flickable.html)のcontentX/contentYを使う。
番組詳細はdelegateそのものではなく選択番組のデータを保持し、列が破棄されても表示を維持する。
閉じると既存の親Loaderと表示JSONを破棄する。上下方向の番組要素は局の当日分を生成するため、
縦方向の可視範囲による生成制限と長時間のメモリー測定は残る。

本体の複数列表示を導入した段階で、mainとのUI一致は未完了。上部の日付操作・余白・
ウィンドウ操作、ジャンル配色、詳細ポップアップの配置・アニメーションと視聴操作、
Shift+ホイールの横移動、同時放送局の絞り込みは続けて移植する。


検証: Rust全体68件成功・外部TS依存1件未実行後、追加の複数局・日境界・別network照合試験も
成功。Clippy全ターゲット、fmt、qmllint、releaseビルド成功。番組表Qt試験は5件成功
（初期化・終了を含む）。日変更時の一時的な負の時間幅を見つけて補正し、警告も失敗に
する条件で再検証した。America/New_Yorkの23/25時間の暦日条件でも5件成功。

30局の生成データでは先頭列と最終列のLoaderの生成・解放、列破棄後の詳細保持、日変更での
詳細破棄を確認した。1440×900の描画も確認。Git対象外のguide-grid-fixture.pngと
benchmark/viewing-design/tst_guide_preview.qmlが証跡。画像保存APIの戻り値を真偽判定した
最初の採取試行は失敗し、画像保存とテストの判定を分けて再実行した。

実アプリの通常放送はPLAYING・正常終了したが、外部クリック2方式では番組表が開かず、
guide-grid.pngとguide-grid-xtest.pngはいずれも再生画面だった。実データでの番組表表示を
成功扱いにしない。現在の描画確認は生成データによるQt試験であり、実アプリの開閉・
選局併用・長時間測定とmainとの同一データ画像比較が残る。


## ジャンル配色

mainのviewer-core/channels.rsとQMLのguideColorに合わせ、最初のgenres[].lv1だけを
番組表の配色へ使う。genre.rsのGenre enumはニュースから福祉までの12分類とUnknownを
表す。JSON入力の配列はSerde Visitorで先頭の大分類を読み、残りをIgnoredAnyで消費する。
ジャンルVecやサブ分類を番組ごとに保持しない。
[SerdeのDeserialize実装](https://serde.rs/impl-deserialize.html)を参照した。

未取得・空配列はUnknown、12以上の有効なu8値もUnknownに正規化する。
負数・u8範囲外・先頭項目のlv1欠落・配列以外は既存のEPG解析エラー経路へ返す。
Qtにはgenreという単一の数値を渡す。入力のgenresと出力のgenreは異なる表示用契約であり、
このJSONをMirakurunの入力形式へそのまま戻す用途はない。

Genre自体はrepr(u8)だがProgram全体の容量には配置のpaddingが影響するため、
EPG_MEMORYのrecord_capacity_bytesで構造体全体を引き続き計測する。
Qtはmainと同一の12色を使い、15・欠落・nullは灰色とする。
この追加によってジャンル配色は実装されたが、上部操作と詳細のデザイン一致、
実アプリの番組表操作と長時間メモリー検証は引き続き残る。

検証はEPG関連Rust16件成功。続いてジャンルのJSON出力・欠落時のUnknownを追加した
複数局試験も成功。Qt番組表試験5件、Clippy全ターゲット、fmt、qmllint、
releaseビルド成功。実描画delegateのニュース色と未知・null・欠落時の灰色を確認した。


## 日付選択部品の移植

GuideDateSelector.qmlを追加し、従来のComboBoxをmain相当の日付タブへ置換した。
1280px未満は202×40pxの前日・翌日操作、広い場合は最大572×40pxの7日タブ。
「今日」は62px、後続日は84px、背景・枠・選択色・角丸・16pxの矢印はmainに合わせる。
矢印SVGもmainから追加し、RustビルドのQtリソースに登録した。
日本語表示はmainの翻訳書式M/d（ddd）を使う。動的言語切り替えはまだ未移植。

日付計算は既存calendarDaysの暦日を受け取るので、24時間の固定加算へ戻さない。
部品はindexの要求だけを通知し、EPG取得と状態の所有は変更しない。
端のボタンは無効にし、範囲外・同じ日の要求は送らない。
選択位置と幅は170ms OutCubic、狭い表示の日付は70/120msのフェードで更新する。
日付列の選択項目を表示範囲へ戻す処理もmainに合わせる。
キーボードの左右・上下からも同じ選択要求へ接続する。

この段階は日付部品の移植であり、番組表上部全体の84pxツールバーへの統合、
ウィンドウボタン・戻る・設定操作の配置は引き続き残る。

検証: 最初のQt試験で未同梱SVGを検出して修正。修正後の番組表Qt試験6件、
qmllint、fmt、releaseビルド成功。前日・翌日の無効化と要求、広いタブのクリック、
既存の暦日・詳細保持の回帰を確認した。実アプリでの日付操作とmainとの画像比較は残る。


## 上部ツールバーの統合

GuideToolbar.qmlにmainの84pxツールバー、18pxの上・左右余白、980pxを境にした
8/14pxの間隔を移植した。戻る・放送種別・日付・設定を1行にまとめ、右端には既存の
WindowButtons、背景にはWindowDragAreaを接続する。ProgramGuideは要求信号を親へ渡し、
Main.qmlが既存SettingsDrawerとウィンドウを接続する。番組表本体は84px直下から始まる。
旧実験UIの更新・閉じるの文字ボタンと多段の見出し行を除き、不要になった更新信号も削除した。
既存のEPG定期更新とキーボード側の更新経路は変更していない。

現在時刻へのスクロールはmainの表示高さ×0.34に合わせる。日付変更で横位置をリセットせず、
放送種別変更時にだけ戻す。現在時刻線は固定の局見出しに重ならないよう表示範囲を制限した。

Qt試験では戻る・設定の信号配送と84pxの予約領域を追加検証した。
別の描画用QML Windowで1440px/900pxのツールバーとウィンドウボタンを確認した。
Git対象外のbenchmark/viewing-design/tst_guide_toolbar_preview.qmlと
 guide-toolbar-wide.png / guide-toolbar-compact.pngを参照する。
最初の採取ではテストランナーのQQuickViewをWindow型へ渡す警告を出したため、
明示的なQML Windowへ変更して再検証した。実アプリでの外部入力・設定操作や
ネイティブウィンドウ移動を確認した証拠ではない。

上部の統合は追加したが、mainとの同一データ・同一フォントの画像比較、詳細ポップアップ、
視聴操作、Shift+ホイール、長時間の資源測定は引き続き残る。

最終検証は番組表Qt試験7件、描画採取3件（初期化・終了を含む）、qmllint、fmt、
releaseビルド成功。スクロール修正後の画像では時刻線が見出しに重ならないことを確認した。


## Shift＋ホイール

mainと同じくShift＋ホイールの角度deltaから左右を決め、1回につき222pxの局幅を移動する。
移動は150ms OutCubicで、開始前に慣性移動と以前のアニメーションを止める。
左右端へclampし、放送種別変更で局一覧が変わった場合もアニメーションを止めて左端へ戻す。
新たなモデルやキャッシュ、継続Timerは追加しない。

重ねたMouseAreaは通常のwheelを拒否してFlickableへ渡し、押下・クリックも拒否して
番組セルへ渡す。[MouseAreaの伝播仕様](https://doc.qt.io/qt-6/qml-qtquick-mousearea.html#propagateComposedEvents-prop)
を参照し、mainと同じpropagateComposedEventsとscrollGestureEnabled=falseを使う。
既存の番組クリック・詳細保持試験を残し、QtのmouseWheelイベントで左右1列移動、
通常ホイールの縦移動、端のclamp、移動中の放送種別変更を検証する。
実マウス・タッチパッドのOS経由入力は別途確認が必要。

変更後の番組表Qt試験8件、qmllint、releaseビルド成功。実OS経由のホイール入力と
長時間操作時の資源測定は未実施。


## 放送中番組からの視聴要求

番組詳細にmainの168×44px・角丸22・アクセント色の視聴ボタンを追加した。
表示は現在放送中の番組だけで、開いている間は1秒ごとに時刻を更新する。
ProgramDetailsの視聴機能は明示的に有効化した番組表だけで使い、他の詳細表示には追加しない。
現在の詳細パネル本体はまだ旧実験用の中央Popupであり、mainの位置・大きさ・
アニメーションへの置換が残る。

watch.rsのIdentityは配信用endpoint、明示的BroadcastService、番組id・開始・durationを
持つ。番組表のwatchKeyはこの値をJSON文字列として表し、Qtの数値へu64 IDを渡さない。
グリッドのCellはProgramを借用してキーの数値フィールドだけを持ち、本文を複製しない。
キー文字列はシリアライズ時の一時領域とQtの当日モデルに増える。履歴や新しいSnapshotは持たない。

Player::watch_programはEPG有効・番組表表示中・有効な時計を検査する。
watch_channelは最大512バイトのキーを解析し、現在の局一覧のendpointと放送情報を照合、
現在番組のid・開始・durationが一致するときだけ今のindexを返す。
行の並び替えに依存せず、終了・番組更新・消滅した局を型付きエラーにする。
成功なら番組表を閉じて既存select/play経路へ進み、失敗なら詳細内にエラーを示して選び直せる。
再生開始そのものの失敗は既存のPlaybackエラー画面へ渡す。

CPU試験はu64::MAX、別networkの同serviceId、並び替え、開始・終了境界、endpoint変更、
番組置換、長すぎるキー・壊れたキーを検証する。Qtは文字列の変更なしの配送、
前後の時間でボタンが消えること、再検証エラーの表示を確認する。

変更後のRust全体試験は71件成功、外部TSを必要とする1件は未実行。
番組表Qt試験9件（初期化・終了を含む）、qmllint、fmt、全ターゲットClippy、
releaseビルドが成功した。新しいQtメソッドの追加で判明した増分ビルド時の
起動クラッシュも修正し、実放送の再生開始・正常終了まで確認した。
原因と古い生成情報を使った再現・回帰検証は[qml-build-order.md](qml-build-order.md)に記録した。
実アプリで番組表の視聴ボタンを押して選局する一連の操作と、長時間の資源測定は未確認。

## 番組表専用の詳細カード

GuideProgramDetails.qmlへ番組表の詳細表示と視聴要求を分離した。
汎用ProgramDetails.qmlから番組表専用の時刻監視・視聴操作を除いた。
mainのguideDetailを照合し、幅500・高さ360px、角丸18px、余白28px、間隔14px、
背景・境界・文字色、22px最大3行のタイトル、局名、区切り線、88pxの説明を移植した。
長いタイトルや再検証エラーで操作がカード外に出ないよう、説明欄を必要な分だけ縮める。
説明を省略しても足りない場合はタイトルの高さも制限し、省略表示にする。
小さい表示領域ではカード自体も縮める。最低ウィンドウ寸法未満の表示は保証しない。

GuideTimelineは選択時にセル座標をmapToItemで番組表本体の座標へ変換し、
番組・座標・局名を渡す。破棄されるセル自体を保持しない。
mainと同じく左側の局なら右、右側の局なら左へ表示し、左右24px・上下20pxに収める。
カードを開いた後のスクロールでは選択時の座標を維持し、領域縮小時には再度clampする。

[Qt Popupの仕様](https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#popup-positioning)
に従い、parentを番組表本体にして相対座標を渡す。非modal・dim=falseで、
main同様に背景を暗くせず上部ツールバーの操作を許可する。
背後の番組表セルへの入力だけをMouseAreaで遮断する。
enter/exitでopacityを150ms OutCubic、scaleを180ms OutBackで変化させ、
Escape・外側クリックによる終了アニメーション完了後にLoaderを解放する。
日付変更・機能終了では即時解放する。開いているカード1枚以外の履歴は追加しない。

描画用のテストデータを使って1440×900と640×480の画像を採取し、
長い番組名・エラー・視聴ボタンの配置を確認した。証跡はGit対象外の
benchmark/viewing-design/tst_guide_detail_preview.qml、guide-detail-wide.png、
guide-detail-error.png。これは実放送の番組情報でもmainとの画像差分検証でもない。

番組表Qt試験11件が成功。座標の左右切替、領域縮小、Escapeの終了アニメーション中の
Loader保持と完了後の解放、ツールバー操作、外側クリックを検証した。
その後のタイトル高さ制限は長文とエラーを加えた視聴操作試験で再検証した（3件成功、
初期化・終了を含む）。qmllint、fmt、releaseビルドも成功。
実アプリでの番組表操作とmainとの同一条件画像比較、長時間資源測定は引き続き未確認。


## 表示エラーの保持と再翻訳

番組表JSONの投影失敗をepg_statusへの一回の文字列設定から、guide_errorの
Option<serde_json::Error>へ変更した。取得状態の定期再投影や言語変更で失敗が消えない。
表示成功、日付変更、開閉で解除し、失敗履歴は蓄積しない。表示状態の再投影は番組表更新の後に行う。
現在の通常データで投影失敗を再現したものではなく、失敗分岐の上書きをコード上で修正した。

番組詳細からの視聴要求がUnavailable/NotLiveとなる場合は、Qt境界で固定の英語sourceへ
変換し、エラーLabelがBackendコンテキストで翻訳する。エラーが表示中でも言語設定に追随する。
低層のエラー型や不透明な番組IDの再検証方式は変えない。

EPG関連12試験、QML静的検査、QtCore/QML翻訳・カタログ欠落試験（187項目）、
全ターゲットClippyが成功。追加した案内の実画面表示とエラー保持の実GUI試験は未実施。

fmt・diff検査とCMakeリリースビルドも成功。


## 番組表JSONの中間配列を除去

Snapshot::grid_viewで全列のVecと各列のVec<Cell>を収集してからJSON化していた処理を、
grid.rsの借用ビュー（Grid / Column / Programs / Cell）へ分離した。
[Serde Serializer::collect_seq](https://docs.rs/serde/latest/serde/ser/trait.Serializer.html#method.collect_seq)
でイテレーターを直接シリアライズする。番組レコードの複製も全セル分の中間Vecも作らず、
一つのSnapshotと選択日の条件を借用して処理する。

列順・番組順、日付境界の重なり判定、空列、ジャンル、watchKeyの形式は維持する。
最終JSONのString、各watchKeyの一時String、Qt側のJSON解析・表示メモリーは残る。
この変更だけでRSSや長時間再生のメモリー上限が保証されるわけではない。

既存の日付境界・サービス識別・u64最大IDの試験に加え、251番組を一列へ渡す試験で
全件の順序と各watchKeyの番組／サービス配信用IDが保たれることを検証する。

EPG関連12試験、全ターゲットClippy、fmt・diff検査、CMakeリリースビルドが成功。実GUIでのRSS差は未測定。


## 選局時の不要な全局JSON生成を除去

全局グリッドの生成入力はSnapshot・局一覧・DayWindowであり、選択局は含まれない。
旧来の単一局表示の更新条件だったguide_serviceをPlayerから削除し、選局だけでは
全局グリッドを再生成しないようにした。EPGのrevision更新、局一覧変更によるguide_dirty、
表示日の変更／再オープンによるguide_dirtyは引き続き再生成を要求する。

現在番組の表示には従来どおり選択局を渡す。QMLのchannelプロパティも選択局に直接
バインドされ、番組表内で開いた詳細はonChannelChangedで閉じるため、選局のUI反映を
グリッドJSON更新に依存させない。キャッシュや共有所有権は追加していない。
JSON生成とQString変換の不要な実行を除く変更であり、Qt setterが同一値を通知するかどうか
にかかわらずRust側の処理を省ける。実操作時のRSS・実行時間の改善量は未測定。

EPG関連17試験、Clippy全ターゲット、fmt・diff検査が成功。試験は全局／日付投影、
更新・キャンセル、現在番組等を対象とし、実画面の選局操作を自動実行したものではない。

CMakeリリースビルドも成功。全機能併用時の実測は引き続き必要。


## 実Mirakurun応答のCPU検証

2026-09-07、設定済みサーバーの/api/servicesと/api/programsを読み取り、双方HTTP 200を
確認した。局一覧14,953 bytes・69件、EPG 10,074,425 bytes・14,602件。
取得時に局一覧1MiB／EPG32MiBの上限と10秒のソケットタイムアウトを設定した。
データは一時ディレクトリー `/tmp/viewer-mirakurun-check-nog1ye6x` のみに保存し、Gitへ含めない。

任意実行の `validates_captured_server_catalog_and_guide` 試験を追加した。
アプリの局フィルター・EPGパーサー・当日のJST日付範囲の全局投影へ取り込み、
列数／列indexと全セルの視聴キーのデシリアライズを確認する。fixtureの読み取りも
アプリと同じバイト上限で制限する。ネットワークや表示装置を試験自身は使用しない。

```sh
MIRAKURUN_CAPTURE_DIR=/tmp/viewer-mirakurun-check-nog1ye6x \
CARGO_TARGET_DIR=build/cargo cargo test --locked --manifest-path rust/Cargo.toml \
  validates_captured_server_catalog_and_guide -- --ignored --nocapture
```

同じ応答から視聴対象54局・EPG14,602件を解析し、54局の現在番組、当日グリッド1,402件、
出力JSON632,023 bytesを確認した。EPG容量内訳はレコード配列1,703,936 bytes、
番組文字列1,652,998 bytes、音声情報494,226 bytes、合計3,851,160 bytes（約3.67MiB）。
これは既存record_storageの容量集計であり、HTTP本文・allocator管理領域・Qt/QML・
動画再生のメモリーを含むRSSではない。時刻を使うため当日件数は試験日時に依存する。

実サーバーの応答スキーマとの互換性を確認する検証であり、Qt画面の反映、EPGイベント、
再生・音声出力・長時間併用の検証を完了したものではない。

実応答の任意試験、Clippy全ターゲット、fmt・diff検査が成功。本体の変更はなく、
リリースの再ビルドは行っていない。通常試験のignoredは外部入力／測定用の3件となる。


## 容量計測値の型と出力の分離

Snapshot::record_storageは合計容量をログへ出す一方で文字列容量だけを返しており、
実データ検証の出力ラベルを誤りやすい形だった。Snapshot::storageへ変更し、
Storage { records, strings, audio }の各バイト容量とtotal()を明示的に参照する。
モデルは値を返すだけとし、EPG_MEMORYログは取得結果を採用する側で出力する。
既存のtext_capacity_bytesには従来どおりstringsを代入し、診断値の意味を変えない。

集計の頻度と走査回数は変えず、新しいキャッシュやヒープ割り当ては追加しない。
実応答検証も同じStorageの内訳を出す。EPG関連17試験が成功し、実応答指定が必要な
任意試験は通常実行ではignoredとして扱う。

保存済み実応答の任意試験も成功し、records=1,703,936、strings=1,652,998、
audio=494,226、total=3,851,160 bytesで変更前と一致した。Clippy全ターゲット・
fmt・diff検査も成功。これは保持量削減ではなく計測APIと責務の整理である。
CMakeリリースビルドも成功。実GUIでの計測表示と長時間検証は引き続き残る。


## main実画面比較と同時放送列（2026-09-07）

main 2d5d15cをビルド確認（更新不要）し、専用Xvfb :99 / llvmpipeで、通常設定と
分離した同一接続先・選局・言語・ウィンドウサイズの設定を使って番組表を比較した。
mainはNHK総合1京都・大津に続いてEテレ大阪を表示したが、実験版はその間に
同時放送のNHK総合2等の列を表示していた。mainのbuild_channelsが局カタログで
適用する除外規則を、実験版の番組表は適用していなかった。

ProgramInfo::visible_channelsへ既存規則を共通化し、前後選局も同じ関数を使う。
番組表を開いている間は既存の現在番組更新（1秒間隔）で現在放送を照合し、
局インデックスの小さいJSONだけをQtへ渡す。EPGスナップショット、番組表全体のJSON、
局カードの番組要約はこの更新で複製・生成しない。判定用の一時集合と局数分の配列は
更新ごとに解放される。番組表を閉じると投影を解除し、計測用Timerも追加しない。
表示対象が変わったら古い詳細を閉じ、列は元のカタログindexを保持する。
表示待ちはnull、取得済みの空一覧は[]として区別する。

Rustの番組境界で同時放送→別番組へ変わる試験と既存EPG試験は17件成功・外部データ試験1件除外。
QMLは元の局番号保持、放送種別、列の復帰・除外、詳細解除を含め73件成功。
アプリ全ターゲットClippyも成功。main側は閉じるボタンから終了コード0。
証跡はgit管理外benchmark/ui-comparison/mainのreference.png、guide.png、run.log。

下部60pxの案内欄、現在時刻の左端バッジ、停止画面の見出しの書体・文言等は、
今回の比較で残る差として確認した。番組表全体の見た目の移植完了とは扱わない。

releaseビルド後の実アプリでも、同じ1440×900の番組表でNHK総合1京都・大津、
Eテレ大阪、びわ湖放送、MBS、KBS京都の順に表示され、比較したmainの列構成と一致した。
同時放送サブ局の余分な列は表示されない。画像はbenchmark/ui-comparison/guide-visibility-fixed.png、
ログはbenchmark/virtual-ui/guide-visibility.log。これは現在の放送構成の確認であり、
実放送での別番組開始による列の復帰はRust/QML試験とは別に検証が残る。


## 現在時刻バッジと停止画面の見出し

mainの番組表にある左端の現在時刻バッジ（x=20、68×24、角丸12、accent色、
11px太字）を移植した。既存のnowからcurrentTimeYを一度求め、時刻の線とバッジで共有する。
バッジはFlickableの外なので[contentY](https://doc.qt.io/qt-6/qml-qtquick-flickable.html#contentY-prop)
を引き、中央を線へ合わせる。描画領域外は既存の時刻軸Itemでclipする。
新しいTimerや番組JSONは追加しない。日付の判定は既存のdayEndを使い、24時間固定にはしない。

QML試験で垂直スクロール前後の線とバッジの実座標一致、選択日の開始前・終了時刻での
非表示を確認した。全体74件成功・警告なし。停止画面の見出しもmainのLive TVと
既存の日本語カタログ、32px太字（エラー時26px太字）へ揃えた。

releaseビルド成功後、専用Xvfb画面で「ライブテレビ」の見出しと現在時刻バッジを
確認した。ホイール操作後もバッジ中央と線が揃って移動した。
画像はbenchmark/ui-comparison/stopped-heading-fixed.png、guide-clock-fixed.png、
guide-clock-scrolled.png。ログはbenchmark/virtual-ui/guide-clock.log。
