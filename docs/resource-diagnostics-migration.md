# 継続的な資源診断の移植

mainのrust/src/diagnostics.rsを参照し、viewer-diagnostics crateへ計測と保存を分離した。
この段階ではアプリ本体から呼んでおらず、定期記録・受信キュー・終了待ち・Qt GC通知・
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
