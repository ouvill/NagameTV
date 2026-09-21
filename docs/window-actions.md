# ウィンドウ操作と全画面（過去の検証記録）

> [!NOTE]
> 本ドキュメントの内容は **[ショートカットとウィンドウ・キー操作](shortcut-actions-design.md)** に統合されました。  
> 現在のアーキテクチャ、全画面切り替え、Escキーの優先順位、キーバインドの最新仕様は統合先のドキュメントを参照してください。

以下は、ウィンドウ操作・全画面機能の実装および検証時の歴史的な記録です。

---

## 過去の実装・検証履歴 (2026-09-07 〜 2026-09-18)

`ViewerActions.qml` が全画面・パネル開閉を共有し、`ShortcutBindings.qml` がキーを登録する構成へ移行した。  
`InputContext.qml` が文字入力・ポップアップ・Slider等の受付条件を判定する。

### APIの根拠
- [Qt Quick Window](https://doc.qt.io/qt-6/qml-qtquick-window.html): visibility, activeFocusItem, showFullScreen/showNormal/showMaximized を使用。
- [Qt Quick Shortcut](https://doc.qt.io/qt-6/qml-qtquick-shortcut.html): contextとautoRepeatを明示。

### 検証（2026-09-07）
`rust/qml/tests/tst_WindowActions.qml` は実際のApplicationWindowを作成し、キー入力、文字入力へのGの配送、無効なEPG、ポップアップを先に閉じるEscape、通常／最大化から全画面を経由して元の状態へ戻る操作を検証。  
X11とNVIDIA OpenGL環境でQMLテスト19件、Rustテスト48件成功。

### 閉じたDrawerによるショートカット抑止の修正（2026-09-07）
設定Drawerが閉じていても `WindowActions.popupOpen` が true となり、各種キーや自動非表示が抑止される不具合を修正。Qt 6.10の `Overlay.visible` 依存から、公開children内のvisible判定へ変更。

### 番組表と局一覧が重なった場合のEscape（2026-09-07）
前面にある番組表（z=500）が背面にある局一覧（z=6）より先に閉じるようEscapeの判定順序を修正。
