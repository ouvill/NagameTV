# 継続的な資源診断の移植

mainのrust/src/diagnostics.rsを参照し、viewer-diagnostics crateへ計測と保存を分離した。
この段階ではアプリ本体から呼んでおらず、定期記録・Qt GC通知・
終了済みプロセスのログ整理は未移植。mainのSnapshotの項目を揃えて接続する必要がある。

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
