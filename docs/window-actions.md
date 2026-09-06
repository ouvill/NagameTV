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
操作部の自動非表示、mainの独自ウィンドウ枠、チャンネルブラウザーはまだ未移植。

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
