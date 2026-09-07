# 全画面とショートカット

`WindowActions.qml` が対象WindowとQtのウィンドウ状態を扱い、番組表・選局・Escapeは
操作要求のsignalとして通知する。Mainが画面の開閉順序とRustへの選局要求を組み立てる。
Rustの再生パイプラインにウィンドウ状態を持たせない。

F11と全画面ボタンは同じ関数を使う。全画面に入る前の状態をQtの列挙値で記録し、
解除時に通常または最大化へ戻す。Windowのvisibilityを表示状態の根拠とし、
独立したfullscreenフラグは持たない。終了時にはショートカットを無効にする。

GはEPGが有効な場合に番組表を開閉する。PgUp/PgDownは前後の局を要求する。
TextInput/TextEdit（TextField/TextAreaを含む）にフォーカスがある間と
ControlsのOverlay上にポップアップがある間は、これらのナビゲーションを抑止する。
Escapeはポップアップ自身のclosePolicyを優先し、その後はMainが番組表、動画統計、
全画面の順に閉じる。各ShortcutはWindowShortcut、キーリピートなし。

このモジュールはWindowと同じ寿命で、タイマー、通信、映像バッファー、履歴を追加しない。
全画面で描画領域が変わった際のQt/GPU側の確保量は別途実測の対象となる。
操作部の自動非表示は [overlay-visibility.md](overlay-visibility.md) を参照。
mainの独自ウィンドウ枠の移植と未検証事項は後述。
チャンネルブラウザーのC開閉は [channel-browser.md](channel-browser.md) を参照。

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
