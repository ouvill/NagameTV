# 継続的な資源診断の移植

mainのrust/src/diagnostics.rsを参照し、viewer-diagnostics crateへ計測と保存を分離した。
アプリ本体へ定期記録・Qt GC通知・終了済みログ整理を接続した。
以下は段階ごとの実装記録で、現在の接続状況は末尾を参照。

## 計測

measurementはmainと同じglibc統計と/proc/self/status、smaps_rollup、fdを読み取る。
番組名・コメント・ユーザーIDは読み取らない。各/procテキスト入力は64KiBまでとし、
取得失敗、不正な数値、Private_Clean＋Private_Dirtyの加算オーバーフローはNoneで保持する。
ゼロの計測値はSome(0)として区別する。FD走査が途中で失敗した際も不完全な個数を返さない。
計測のために開いたFDディレクトリーの1件を除く点はmainを維持する。

[Linuxカーネルのproc仕様](https://docs.kernel.org/filesystems/proc.html)に従い、
RSS関連のstatus値は非同期集計による近似値として扱う。smaps_rollupはページ表を調べる
ため負荷があり、別時点のstatus・allocator値と完全に一致するとは限らない。
実装をアプリへつなぐ際はGUIスレッドから周期的に読まず、診断ワーカーへ渡す。
mallinfo2もallocator内部ロックを取得しうる。glibc以外ではallocator統計はNone。
これらの値はQtやドライバーの全確保量を説明するものでも、メモリーリークの判定でもない。

## 保存

storageのRotatingWriterは現行区画4MiBと前回区画1個を保持する。mainのJSONL形式と
previous.jsonlへの切り替えを維持する。1レコードは改行・JSONエスケープ後を含め64KiBまで。
serde_jsonの出力を上限付きWriteへ流し、大きいVecを作り終わってから検査する方式を避ける。
上限超過は型付きエラーとして返し、現在のログを書き換えない。
既存の現行ログが4MiBを超えていた場合もエラーとし、巨大ファイルを前回区画へコピーしない。

IOエラーの後はワーカーを終了させる契約とする。ディスク障害やプロセス強制終了で
途中の行が残る可能性はある。通常の区画切り替えではJSONの途中で行を分割しない。
この2区画上限は1プロセス分であり、全実行履歴の容量上限ではない。mainの終了済みログ
整理と他の稼働中プロセスを削除しない仕組みを別途移植する。

## 検証

5件のCPUテストで、現在のプロセスのRSS・スレッド・FD・glibcの取得、欠損と不正値、
数値オーバーフロー、入力サイズとUTF-8、区画切り替え後の連続した末尾レコード、
JSONエスケープによるサイズ超過と既存ログの保持を確認した。
全ターゲットClippyとfmtも成功。表示・GPU・音声は使用していない。アプリとの併用負荷、長時間動作は未検証。

```sh
CARGO_TARGET_DIR=build/diagnostics cargo test --locked --manifest-path rust/crates/viewer-diagnostics/Cargo.toml
CARGO_TARGET_DIR=build/diagnostics cargo clippy --locked --manifest-path rust/crates/viewer-diagnostics/Cargo.toml --all-targets -- -D warnings
```


## 有限キューと終了待ち

Recorderを追加し、mainと同じ上限32件のsync_channelへtry_sendする。
Snapshotはmainと同名の固定個数の数値・真偽値だけを所有する。イベントはEvent enumで
許可された名前へ限定し、任意の長い文字列や番組・実況の本文をキューへ渡さない。
受理・満杯による破棄・停止済みはEnqueue enumで区別する。満杯ではブロックせず、
AtomicU64の破棄件数を以降のレコードへ記録する。終了済みキューへの要求は破棄数に含めない。

出力ファイルのオープンとワーカー生成は起動時に行い、/procとallocatorの計測、JSON生成、
ファイル書き込みは専用スレッドで実行する。独自タイマー・Tokio runtimeは作らない。
記録要求時刻と実際の計測時刻を別に保持し、混雑時に両者がずれたことを確認できる。

stopはRecorderを消費して送信口を閉じ、Stoppingを返す。キューに受理された最大32件と
処理中の1件をワーカーが処理して終了する。Stopping::is_finishedで終了を確認でき、
joinは保存エラー・ワーカーパニックをResultで返す。IOエラーの後は処理を終え、
後続レコードを途中まで壊れたファイルへ追加しない。

明示stopを忘れた場合もDropは送信口を閉じてjoinし、スレッドを切り離したまま残さない。
このフォールバックとStoppingのDropはブロックする。低速・停止したファイルシステムへの
書き込みを強制キャンセルする機能はなく、終了時間の上限は保証しない。アプリ側では
stop→終了確認→joinの順に扱い、正常終了時に保存エラーを取り出す必要がある。

書き込みを明示的に止めた状態で32件だけ受理し、その後1000件を破棄する試験、
停止後の全受理分保存、join復帰前の最終JSON保存、保存失敗とパニックのResult化を追加した。
合計9件とClippy全ターゲット、fmtが成功。実アプリの定期記録・設定への接続、
Qt GC通知、終了済みログ整理、長時間測定は依然として未実施。


## 終了済みログの整理

retentionを追加し、mainと同じusage-<pid>.jsonlとusage-<pid>.previous.jsonlのうち、
終了が確認できるプロセスの新しい6区画を保持する。6プロセスではなく6ファイルである。
候補は最小ヒープに最大7件だけ保持し、ディレクトリー内の全パスをVecへ蓄積しない。
更新時刻が同じ場合はパス順で決める。通常ファイル以外、PIDが0・非数値・非正規表記、
別名ファイルは対象外。シンボリックリンク先のファイルをログとして追跡しない。

Linuxで/proc/selfを確認でき、対象/proc/<pid>がNotFoundの場合だけ終了済みとする。
存在するPIDは他アプリのPIDへ再利用された場合も保持する。procfsの不在・権限不足や
非Linux環境はUnknownとし、自動削除しない。この点はmainの非Linux時の扱いより保守的。
削除前にPID状態・通常ファイルであること・更新時刻を再確認する。これらは別々の
ファイルシステム操作であり、任意の外部書き換えとの原子的な排他を保証するものではない。
稼働中のアプリはPID名のファイルを使用し、そのPIDを観測できる場合は候補にしない。

Recorder::start_directoryはディレクトリーを用意し、整理後にmainと同じPID名で開始する。
同じプロセス・同じディレクトリーで同時に複数のRecorderを作らない契約。
まだアプリから呼び出しておらず、ユーザーの実ログには整理処理を実行していない。

保持順と6区画上限、稼働中・不明のPID、削除直前に稼働状態へ変わる候補、
シンボリックリンクと無関係なファイルの保持を一時ディレクトリーで検証した。
単独crateの合計13件、Clippy全ターゲット、fmtが成功。Qt GC通知とアプリへの接続、
実アプリの長時間資源計測は引き続き残作業。


## GC通知の受け口

GcCategoryでqt.qml.gc.statisticsとqt.qml.gc.allocatorStatsだけを受け付け、その他の
文字列はparse時にNoneとする。GcSinkはWeak参照で保持でき、Qt側のコールバックより
Recorderが先に終了しても送信口を保持し続けない。本文はmainと同じ4096 Unicode scalar
まで取り込み、最大16KiBのUTF-8本文として所有する。JSONエスケープ後も既存のレコード
64KiB上限を適用する。通知スレッドではファイル書き込み・プロセス計測を行わない。

GCと通常Snapshotは同じ32件キューを使う。送信口の短い排他はtry_lockし、競合時も
待機せずDroppedと破棄件数を返す。満杯時も同じ扱い。ワーカーではGCレコードをmainと
同じkind/schema/pid/unix_ms/category/message/dropped_recordsで保存し、GC通知の
たびに/procやallocatorを追加計測しない。

stopは送信口そのものを閉じる。直前にWeakをupgradeした通知処理が残っていても、
その参照の解放を待つことなくワーカーは受理済みキューを処理して終了できる。
停止側は短い送信処理の排他完了を待つ場合があるが、その排他中にIOや計測は行わない。
poison状態は記録の継続に使わず、送信口を閉じる目的だけで内部所有権を回収する。

非BMP文字・改行・引用符を含む長文の上限とJSON復元、停止後に残る通知参照、
競合中の非待機・破棄件数、Snapshotで満杯のキューへのGC通知を検証した。
合計15件、Clippy全ターゲット、fmtが成功。これはRustの受け口を直接呼んだ試験であり、
Qtメッセージハンドラーの登録と実GC通知、アプリの定期記録への接続はまだ未実施。


## アプリへの接続

診断ディレクトリーはmainと同じStateLocation内のusage。通常起動は既定で有効、
MIRAKURUN_DIAGNOSTICS=0で無効にする。比較実験の--features指定時は既定で無効にし、
MIRAKURUN_DIAGNOSTICS=1の明示指定でのみ有効にする。無効時はRecorder・usageディレクトリー
整理を生成しない。再生エラー単体の保存は従来どおり別機能。

QMLから起動時・10秒ごと・パネルや機能設定変更時にrecord_ui_stateを呼ぶ。
再生・停止・選局、EPG取得開始／成功／失敗も対応するEventを渡す。
表示側のUiStateは前回のフラグと描画中の実況数だけを保持する。GUIからは固定項目の
Snapshotを渡すだけとし、従来のGUI内/proc・mallinfo2定期取得を削除した。
従来のstderr ALLOC計測行とGUIのRSS表示に代わり、JSONLのprocess/allocatorへ記録する。
起動時のM_MMAP_THRESHOLD設定とその説明・ログは維持する。

EPG文字列容量は取得成功時の既存record_storage集計から保存し、定期記録で全件走査しない。
無効化・サーバー変更でゼロへ戻し、取得失敗時は表示に使う旧データの値を維持する。
実況履歴は最大200件のBox<str>本文長を合計する。mainのtime/source文字列は新実装で
保持しないため、その分を架空の容量として加算しない。字幕は表示へ投影したセル数を
保持し、クリア・停止・表示設定変更でゼロへ戻す。pending値の取得失敗はNoneとする。
既存commentsは画面表示設定を表し、受信機能のcomments_enabledとepg_enabledを追加した。

[Qtメッセージハンドラー仕様](https://doc.qt.io/qt-6/qtlogging.html#qInstallMessageHandler)
に合わせ、Qt/GStreamerの初期化前に一度ハンドラーを登録する。mainと同じ2カテゴリを
RustのGcSinkへ渡し、既存ハンドラーがあれば通常メッセージを引き継ぐ。
C++側は8192 UTF-16 code unitまで、Rust側は4096 Unicode scalarまでとする。
MIRAKURUN_GC_LOG=1でカテゴリを有効にし、QT_LOGGING_RULES/CONFの優先順位は
[QLoggingCategoryの仕様](https://doc.qt.io/qt-6/qloggingcategory.html#setFilterRules)に従う。
明示的に有効化しなければ通常のカテゴリ設定を変更しない。

正常終了では送信口を閉じてjoinし、受理した記録を処理した後に戻る。ディスクIOが遅ければ
終了が遅れる可能性は残る。処理中にワーカーが終了した場合は次のUI記録要求時に結果を取り出し、
保存失敗をlog_errorへ表示する。GC登録はWeakのみを保持し、終了後の通知を捨てる。

EPG更新・失敗時保持・無効化のテストに容量カウンターの検証を加えた。12件が成功。
診断の既定値・比較実験・明示指定のテスト1件も成功。単独診断crateの15件が成功。
実アプリでのGC発生、定期ログ生成、画面上のカウンターとの照合、長時間測定は未実施。
音声接続問題の解消確認がないため、今回も実アプリは起動していない。

組み込み後の全ターゲットClippy、fmt、CMakeリリースビルドも成功。
