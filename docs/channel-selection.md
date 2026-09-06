# チャンネル分類と選局の移植

mainの `viewer-core/src/channels.rs` とMirakurunの
[Service実装](https://github.com/Chinachu/Mirakurun/blob/master/src/Mirakurun/Service.ts)
を参照し、サービス一覧を解釈する処理を `channels` モジュールへ分離した。
HTTP通信とキャンセルは既存の `services`、Qt向けの変換は `player/channels` が担当する。

## 順序と選択

- 放送種別は `Band` enum（GR、BS、CS、SKY、Other）。未知・欠落の種別も一覧から失わない。
- 地デジはremoteControlKeyId、その他はserviceIdで並べる。番号なし／0は同種別の末尾。
- 同番号はmainと同じ表示ラベル、serviceIdの順。最後にendpoint IDで順序を確定する。
- 番号は地デジ2桁、それ以外3桁。欠落は「--」と表示し、endpoint IDから推測しない。
- TV以外、ID 0、空白だけの名称、重複IDを除外する。JSONエラー・TV局なしは型付きエラー。
- 保存した局は一覧位置ではなくu64のサービスIDで復元するので、並べ替え後も同じ局を選ぶ。

QMLへは全局のindex・label・bandを一つのJSONとして渡す。サービスIDはRustが所有し、
JavaScriptの数値精度に依存させない。旧QStringListとの二重保持はしない。
この表示スナップショットはサービス一覧取得時だけ作り、50msのpollや番組更新時には作らない。
ソートは借用文字列を比較し、比較のたびの文字列確保を避ける。

`ChannelSelector.qml` の種別選択は表示だけを絞り込む。再生は局を選んだ時に行う。
絞り込み後も元の一覧indexを渡し、表示上の0番目を別の放送局の0番目と取り違えない。
前／次や設定復元で絞り込み外の局が選ばれた場合は「すべて」に戻して選択局を表示する。
Qt公式の [ComboBox Model Roles](https://doc.qt.io/qt-6/qml-qtquick-controls-combobox.html#combobox-model-roles)
に従ってtextRoleとvalueRoleを明示し、ユーザーのactivatedだけで選局を通知する。
フィルターは一覧の参照だけを持ち、タイマー・ネットワーク・過去の一覧を保持しない。

## 検証

Rustテストで種別優先順、欠落／0のリモコン番号、同番号の順序、応答順の反転、
重複・非TV除外、不正JSON、u64::MAXの保持、並べ替え後の設定復元を確認する。

[Qt Quick Test](https://doc.qt.io/qt-6/qml-qttest-testcase.html)では実際のComboBoxに
キー入力し、絞り込み時に選局通知しないことと、正しい元indexが通知されることを確認する。
空の種別、外部からの選局、再接続による一覧の空化・置換も確認する。
テスト終了時はcreateTemporaryObjectで生成した表示部品を破棄する。

表示・OpenGL環境を検出し、xdpyinfoとglxinfoで動作確認してから実行する。
利用できない場合は実行を止めて報告し、代替レンダラーへ自動変更しない。

```sh
CARGO_TARGET_DIR=build/cargo cargo test --manifest-path rust/Cargo.toml --release --locked
QT_QPA_PLATFORM=xcb QSG_RHI_BACKEND=opengl /usr/lib/qt6/bin/qmltestrunner -input rust/qml/tests -o -,txt
```

## 残っている移植

ロゴ、mainのチャンネルブラウザー、現在番組の表示は未移植。
mainの同時放送サブ局整理には物理チャンネルと現在番組のsignatureが必要なため、
今回のソート段階では重複サービスID以外の局を隠さない。EPG連動の投影で移植する。
全機能併用での長時間メモリー・操作試験は別途必要。

2026-09-07の検証結果: Rust 40件成功（外部TS依存の1件は未実行）、
Qt Quick Test 3ケース成功（初期化・終了を含む表示は5件）、qmllint警告なし。
Clippy全ターゲット警告なし、CMakeビルド成功。
実アプリで実サーバーの一覧取得、番号付き局名と種別選択欄の表示、YADIFでの自動再生、
正常終了を確認した。QMLの型・参照・バインディングエラーはログに出ていない。
実アプリの放送種別切り替えと選局の往復操作は未検証で、操作の検証範囲は上記QMLテスト。
実機証跡はGit対象外の `benchmark/channel-selection/smoke.py`、`smoke.log`、`playing.png`。


## 局一覧の定期更新

mainのapp.rsの300,000ms周期と、QMLが一覧を開く際のrefreshChannels(false)を照合した。
実験版に5分ごとの局一覧更新、一覧表示時の60秒以上経過後の更新、再生失敗画面からの
一覧表示時の強制更新を追加した。既存pollで単調時計の時刻を比較し、専用タイマー・
スレッド・要求キューは増やさない。取得中は強制要求も含め重ねて発行しない。
接続先の再設定開始・終了時に自動更新を無効化し、有効URLへの要求開始で時刻を記録する。
取得失敗時も直ちに連続要求せず、最後の要求からの間隔を適用する。

背景取得はconnect_serverを呼ばず、再生・字幕を停止しない。既存RequestとNetworkの
容量上限・タイムアウト・結果受け取りを使い、失敗時は前の局一覧を保持する。
なお既存の接続先変更時のRequest破棄はabortであり、これは旧タスク終了を明示joinする
新しい仕組みを追加した変更ではない。アプリのruntime終了を含む実操作検証は残る。

初回接続は設定された局がなければ先頭を選ぶ既存挙動を維持する。定期更新では現在局の
IDを探し直し、局が消えた場合は未選択にする。保存されたIDで再登場した局を復元できる。
並び順が変わっても別の放送を選択・再生しない。既に動作中のストリームは継続する。

Channelの全フィールドを等価比較し、同一結果ならQt用一覧・番組表・実況勢いを再投影しない。
変更があるときだけ一覧を置換し、guide_dirty、activity.dirty、現在番組の更新時刻と
ブラウザーの小さな投影キャッシュを無効化する。物理チャンネル情報だけが変わった場合にも
サブチャンネルの可視判定を更新できるようにする。

CPU試験では更新周期・取得中の重複抑制・無効化、局IDの並べ替え・消失・再登場を確認する。
実サーバーの5分経過、再生継続、局構成変更時のUI・EPG追随、長時間の資源測定は未検証。

検証結果: 関連CPU試験3件、全ターゲットClippy（警告をエラー扱い）、CMakeリリースビルドが成功。
