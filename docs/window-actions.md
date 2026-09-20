# 全画面とショートカット

2026-09-18に操作層とキー定義を整理した。
[現在の構成とキーの追加・差し替え方法](shortcut-actions-design.md)を参照。

`ViewerActions.qml`が全画面・パネル開閉を共有し、`ShortcutBindings.qml`がキーを登録する。
`InputContext.qml`が文字入力・ポップアップ・Slider等の受付条件を判定する。
再生／停止／一時停止の選択はRustの共通操作を呼び、画面の状態はQMLで管理する。
局一覧はS、コメント入力欄の表示とフォーカスはCに割り当てる。

F11と全画面ボタンは同じActionを使い、通常または最大化へ復帰する。
Escはポップアップを優先した後、番組表→局一覧→コメント入力→統計→番組情報→全画面解除の順。
全キーはWindowShortcut、左右シークだけキーリピートを許可する。
操作部の自動非表示は[overlay-visibility.md](overlay-visibility.md)を参照。
以下の日時付きの記録は当時の実装・検証環境についての履歴。

## APIの根拠

- [Qt Quick Window](https://doc.qt.io/qt-6/qml-qtquick-window.html)：visibility、activeFocusItem、showFullScreen/showNormal/showMaximizedを使用。
- [Qt Quick Shortcut](https://doc.qt.io/qt-6/qml-qtquick-shortcut.html)：contextとautoRepeatを明示。

## 検証（2026-09-07）

`rust/qml/tests/tst_WindowActions.qml` は実際のApplicationWindowを作成し、
キー入力、文字入力へのGの配送、無効なEPG、ポップアップを先に閉じるEscape、
通常／最大化から全画面を経由して元の状態へ戻る操作を検証する。
警告をテスト失敗として扱う。テスト用ウィンドウの状態は各ケース後に復元する。

X11とNVIDIA OpenGLの動作確認後、QMLスイート19件成功（初期化・終了処理を含む）。
Rustテスト48件成功、外部TSを必要とする1件は未実行。
Clippy全ターゲット警告なし、CMakeビルド成功。

実再生ではEPG有効・字幕無効で、全画面ボタンによる往復を3回実行した。
X11のウィンドウ状態で全画面と解除を照合し、最後の往復で映像表示の画像を確認した。
通常サイズ2200×1440から全画面6880×2880へ移り、元のサイズへ戻った。
再生／QMLエラーなし、WM_DELETE_WINDOWによる正常終了を確認。
証跡は `benchmark/window-actions/` のsmoke.py、smoke.log、fullscreen.png、restored.png。
外部ツールxdotoolからのF11送信では遷移を確認できなかったため、
実アプリのキーボード操作は未確認として残す（Qt Testのキー入力は成功）。
長時間のメモリー・フレーム落ち測定と複数画面間移動は未検証。

## main のウィンドウボタン

`WindowButtons.qml` は main の右上126×42px・角丸21pxのグループを移植する。
各操作は42×42px、SVGは16px。最小化・最大化／通常サイズ・閉じるをQt Windowへ渡す。
全画面状態の最大化操作はmainと同様に最大化へ移り、全画面切り替えは下部操作に残す。
閉じる操作は `Window.close()` を使い、Mainの `onClosing` を経由してRustを終了する。
独自の破棄処理や再生状態の複製は追加しない。
[Qt Window](https://doc.qt.io/qt-6/qml-qtquick-window.html)の状態・close仕様を確認した。

Qt操作試験は最大化から通常サイズへ戻したときの幅・高さ、全画面から最大化、
最小化、閉じるイベントの到達とキャンセル可能性を確認。全体48件成功（初期化・終了込み）。
qmllint警告なし。枠なし表示、ウィンドウ移動・端のリサイズ、実放送を再生しながらの
新ボタン操作はまだ未検証／未移植であり、ウィンドウ装飾全体の再現完了とは扱わない。

## 枠なし表示とシステム操作

MainにQt.Window | Qt.FramelessWindowHintを指定し、mainの76pxの上部ドラッグ領域と
辺10px／角18pxのリサイズ領域を部品へ分離して移植した。ドラッグ領域は操作部品より下、
リサイズ領域は映像・パネルより上に置く。最大化・全画面ではリサイズを無効にする。
上部ダブルクリックは通常／最大化を切り替え、全画面には作用しない。

[QWindowのシステム移動・リサイズ](https://doc.qt.io/qt-6/qwindow.html#startSystemMove)と
[DragHandler](https://doc.qt.io/qt-6/qml-qtquick-draghandler.html)を参照。
押した瞬間には移動せず、プラットフォームのドラッグ閾値を超えてからstartSystemMoveを呼ぶ。
座標の手動更新や最大化復元座標の複製は行わない。Qtが拒否した場合は警告を記録する。
QML Windowの当該APIはQt 6.8以降のため、READMEの実行要件を更新した。

CMakeビルド、qmllint成功。Qt試験49件成功（初期化・終了を含む）。
枠なしテストウィンドウで最大化・元サイズ・全画面・最小化・閉じるイベント、
上部ダブルクリック、全画面時のリサイズ領域無効化を確認した。
実アプリでEPG取得・正常終了には成功したが、xdotoolの外部ドラッグでは位置・サイズが
変化しなかった。XSendEvent/XTestを用いた試行の両方で、入力配送か操作自体かは未切り分け。
したがって実環境の移動・リサイズ成功は未検証として残す。移植完了の根拠には含めない。
証跡はGit対象外のbenchmark/viewing-design/frameless.py、frameless.log、frameless.png。

## 前後選局の候補

PgUp/PgDownはRustのstep_channelを呼び、一覧と同じ同時放送除外規則で巡回する。
一覧が閉じていても操作時のEPGを参照する。番組境界・非表示局からの移動・循環の
検証は [channel-browser.md](channel-browser.md) に記録する。

## 閉じたDrawerによるショートカット抑止の修正（2026-09-07）

ユーザー承認のXvfb :99 / llvmpipeとOpenboxで実アプリを操作した。
設定・診断ファイルはbenchmark/virtual-ui内へ分離し、通常画面:0には入力しない。
ここでの成功はUI検証であり、NVIDIAの描画・メモリー・性能検証ではない。

設定Drawerが閉じていてもWindowActions.popupOpenがtrueとなり、C/G/PgUp/PgDown/
Escapeと操作部の自動非表示が抑止されることを実アプリ内の一時ログで確認した。
Qt 6.10.2のQQuickOverlayPrivate::addPopupはDrawerの登録だけでもoverlayをvisibleに
するため、Overlay.visibleをポップアップ表示中の条件に使うことが誤りだった。
参考: [Qtの該当実装](https://github.com/qt/qtdeclarative/blob/v6.10.2/src/quicktemplates/qquickoverlay.cpp)、
[Popupの表示方式](https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#popup-type)。

Overlayの公開childrenリスト内にvisibleな項目があるかをバインディングで判定する。
閉じたDrawerの登録だけでは抑止せず、実際のポップアップ表示・終了アニメーション中は
抑止を維持する。監視用Timerやイベントフィルターは追加しない。一時ログは除去済み。

閉じたDrawerをWindowActionsの試験へ追加すると修正前は3件失敗した。
修正後は同じ試験6件成功、QML全体72件成功・警告なし。releaseビルドも成功。
実アプリでもCで局一覧、Escapeで閉じる、Gで番組表、Escapeで通常画面へ戻ることを
画像で確認した。新しいウィンドウタイトルの番組名も_NET_WM_NAMEで確認した。
証跡はgit管理外benchmark/virtual-uiのchannels.png・guide.png・closed.pngと各ログ。

F11の外部キー送信はこの実行でも状態変化を確認できていないため、別途調査する。
Qtの部品試験内での全画面往復は成功しているが、実アプリへの外部送信とは分けて扱う。
自動非表示も、この回の実アプリは停止中なので実再生での確認が残る。

## 仮想画面でのF11と再生操作（2026-09-07）

Xvfb :99のxevで確認したところ、xdotool key F11はAlt付き（state=0x8）だった。
前の通常画面ではCtrl付きだったため、送信ツールが選ぶキーの修飾状態を環境ごとに
確認する必要がある。アプリのF11実装を変更する根拠にはしない。
この仮想画面でF11に対応するキーコード95をXTestFakeKeyEventで送ると、
xevではstate=0、実アプリでは_NET_WM_STATE_FULLSCREENへの遷移と、
再送による通常状態への復帰を確認した。検証用send-f11.pyは:99専用で、
他のディスプレイで同じキーコードを仮定しない。

同じ仮想画面の実アプリで以下も確認した。

- 関西テレビで視聴開始後にPipeline PLAYING。操作部が隠れた状態の画像を確認。
- 操作部の表示復帰と停止ボタン操作で停止画面へ移り、字幕購読・待機・受信件数が0。
- 視聴再開後に2回目のPLAYING。ARIB字幕の文字と背景が実映像上に描画されることを確認。
- PgDownで読売テレビ、PgUpで関西テレビへ戻り、それぞれPLAYINGと番組名の更新を確認。
- 最後は実アプリの閉じるボタンで終了コード0。仮想画面の確認用xevも終了した。

EPG・字幕・実況受信は有効、音量は検証用設定で0。実況の新着描画や音声聴取は
この操作記録だけでは未確認。字幕の表示画像も厳密なPTS同期の計測を代替しない。
仮想画面はCPU描画なので、この実行のRSS・CPU・フレームレートをNVIDIA環境の評価には使わない。
証跡はbenchmark/virtual-uiのfixed.log、f11-events.log、send-f11.py、各png。
通常画面のアプリには入力も終了操作も送っていない。


## 番組表と局一覧が重なった場合のEscape（2026-09-07）

mainは番組表を局一覧より先に閉じるが、後継のcloseTopmostは逆順だった。
C→Gで両方を開くと番組表が前面（z=500）、局一覧が背面（z=6）になり、
Escapeが背面だけを閉じて見た目に反応しない不具合を実アプリで再現した。
判定順序を表示順に合わせ、番組表→局一覧へ修正した。

検出・検証済みXvfb :99 / llvmpipe上の停止中アプリで比較。
修正前の診断（guide_open, channels_open）は(false,true)→(true,true)→(true,false)。
修正後は同じC→G→Escapeで(false,true)→(true,true)→(false,true)、
もう一度Escapeで(false,false)。画面でも番組表→局一覧→停止画面を確認した。
修正前後とも閉じるボタンで終了コード0。QML全86件とリリースビルドも成功した。
この順序の直接の検証は実アプリ操作と診断ログであり、既存QML部品試験だけを根拠にしない。

証跡はGit対象外benchmark/escape-order/のbefore/after.log、png、
before-state/after-stateの診断JSONL、after-states.json。
専用設定、字幕・実況無効、EPG有効、明示したfakesinkで実施し、通常画面には入力していない。


## 映像のダブルクリック（2026-09-21）

映像部分の左ボタンダブルクリックで全画面を切り替える。F11と共通の操作を使い、
全画面を解除すると元の通常／最大化状態に戻る。単一クリックは操作部の表示と
テキスト編集の終了に使う。ボタン・スライダー・メニュー上のクリックは各部品が受け取り、
全画面を切り替えない。上端のウィンドウ移動領域は従来の最大化操作を維持する。
