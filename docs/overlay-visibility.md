# 操作部の重ね合わせと自動非表示

mainの再生中3.2秒で操作部を隠す動作を移植する。
映像と字幕の領域はウィンドウ全体から決定し、上下の操作部をPaneとして重ねる。
操作部や番組表の開閉では映像のサイズを変えない。字幕の16:9領域も映像に合わせる。
これはmain相当のUIへ進める変更であり、独自ウィンドウ枠や画面デザインの移植は残る。

## 責務と寿命

- `OverlayVisibility.qml`：再生中・表示固定・操作通知を受け、表示状態と単発タイマーを所有する。
  非再生中、EPG表示中、文字入力中、ポップアップ表示中、音量ドラッグ中は隠さない。
  固定を解除した時点から新たに3.2秒を待ち、終了時はタイマーを停止する。
- `pointer_activity.h`：mainのウィンドウイベント監視を移植。QML itemの子として1個だけ作る。
  Qt Quickのhover配送より前にマウス移動・進入を観測し、イベントは消費しない。
  同位置の通知を除外し、履歴は直前座標1個だけ。ウィンドウ変更時に古いfilterを外し、
  新しいウィンドウをQPointerで参照する。itemの破棄で監視オブジェクトも破棄される。
- Rustの`playing`：GStreamerがPLAYINGを通知した時点でtrue、停止処理の開始時にfalse。
  再接続・選局・停止失敗でも操作部を表示できるよう、停止結果を待たずfalseにする。
  表示文言を再生判定に使わない。Qtの表示状態やタイマーはRustに持たせない。

操作部自体は毎回生成・破棄せずvisibleを切り替える。映像sink、字幕セッション、
EPG取得を自動非表示で再生成しない。番組表のLoaderは従来どおり閉じると破棄する。
診断用の動画統計は操作部とは独立して表示を維持する。

## APIの根拠

- [QObject::installEventFilter](https://doc.qt.io/qt-6/qobject.html#installEventFilter)：監視対象とfilterはGUIスレッド上で扱い、監視のみならfalseを返す。
- [HoverHandler](https://doc.qt.io/qt-6/qml-qtquick-hoverhandler.html)：カーソル形状の切り替えに使う。再表示の通知はmainと同じnative監視を使う。
- [Pane](https://doc.qt.io/qt-6/qml-qtquick-controls-pane.html)：操作部の背景と入力面。映像クリック用MouseAreaより手前に配置する。
- [MouseArea](https://doc.qt.io/qt-6/qml-qtquick-mousearea.html)：映像部分のクリックで入力欄からフォーカスを戻す。

## 検証

`tst_OverlayVisibility.qml` は非再生時の表示維持、再生中の非表示と再表示、
固定中の期限取消し、固定解除後の再計時、終了時のタイマー停止を検証する。
QMLスイート24件成功（初期化・終了処理を含む）。

`scripts/test-pointer-activity.sh` は実際のnative helperを使い、重複インストール防止、
同位置の除外、無効なitemの無視、ウィンドウ付け替え、破棄後のfilter解放を検証する。
Qt Test 3件成功（本体1件と初期化・終了）。表示・GPUの検出と検証後に実行する。

Rustテスト48件成功、外部TSが必要な1件は未実行。Clippy全ターゲット警告なし。

実再生（X11/NVIDIA、EPG有効・字幕無効）で、無操作後の非表示、マウス移動での再表示、
入力欄を選んだ後5秒以上の表示維持、映像クリックで入力を終えた後の非表示、
番組表表示中5秒以上の表示維持、正常終了を確認した。
表示判定はXGetImageで操作部の画素を保存処理前に読み取る。
画像の保存自体に非表示期限以上の時間がかかることがあるため、保存後の画像だけを
瞬間的な再表示の根拠にしない。番組表の重ね合わせは保存画像でも確認した。
証跡は `benchmark/overlay-visibility/` のsmoke.py、smoke.logと画像。
初期案の映像item上のTapHandlerでは実アプリで入力フォーカスを解除できなかったため、
操作パネルの背面に置いたMouseAreaへ変更して同じ操作を再検証した。
実再生の停止／再開と音量ドラッグ中の表示維持はこの変更後には未検証。
長時間のRSS/PSS・フレーム落ち比較、Waylandでの実再生操作は未検証。
