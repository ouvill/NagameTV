# 普段の視聴中の診断記録

次回起動から自動で有効。画面や入力フォーカスを操作せず、ローカルのJSON Linesファイルに記録する。記録内容のネット送信は行わない。

## 保存場所・確認

通常のLinux環境では `~/.local/state/mirakurun-viewer/usage/`。`XDG_STATE_HOME` を指定した場合はその配下。アプリの既存の「ログフォルダーを開く」で開くディレクトリ内の `usage` フォルダー。

- `usage-<PID>.jsonl`: 現在の記録
- `usage-<PID>.previous.jsonl`: 容量上限に達する直前の記録

例（ディレクトリを指定すると最も新しいプロセスのログを選び、直前の記録も集計する）:

```sh
python3 scripts/summarize-usage.py ~/.local/state/mirakurun-viewer/usage
```

そのまま普段どおり視聴し、メモリ増加が気になった時刻を覚えておけば、操作や保持量と照合できる。記録を止める場合は `MIRAKURUN_DIAGNOSTICS=0 ./build/mirakurun-viewer` で起動する。現在稼働している旧バイナリには後付けされない。

## 記録内容

10秒ごと、および選局要求・再生/停止要求・番組表やチャンネル一覧の開閉・字幕/実況の切り替え・EPG取得開始/完了/失敗で記録する。

- 日時、起動からの経過時間、PID、アプリのバージョン、操作の種類
- RES相当のRSS、PSS、private、swap（KiB）、スレッド数、FD数
- 再生、字幕、実況、番組表、チャンネル一覧の状態
- EPGの保持番組数と番組名・説明文字列の確保容量（bytes）
- 実況履歴の件数と保持文字列の確保容量（bytes）
- 現在の字幕のセル数、タイミング調整中の字幕待機数
- 最後にQMLから受け取った表示中の実況件数（Rust側のイベント時点では最大約10秒前の値）

番組名・説明・コメント本文、URL、チャンネルID、キー入力内容は記録しない。選局したという事実は記録するが、どの局かは記録しない。

## 読み取り上の注意

`*_text_capacity_bytes` は指定したRust文字列の容量だけであり、処理全体のメモリ量ではない。Qtの文字列コピー、GPUメモリ、GStreamerバッファ、アロケーターの管理領域などは含まれない。実況の受信待ちキュー件数も、この版ではまだ計測していない。

状態はGUI側で取得し、プロセスメモリは別スレッドで読み取る。`record.unix_ms` と `measured_unix_ms` の差で記録待ちの遅れを確認できる。異なるメモリ項目も厳密に同時ではなく、瞬間的なピークを取り逃すことがある。GUIが停止している間は状態の定期記録も止まる。字幕のロックが使用中の場合や、procfsを読めない場合は該当値を `null` とする。

10秒間隔のため、短い操作単独の割当量を厳密に測定するものではない。増加の傾向と操作の前後を追うための記録。

## 記録自体の負荷・容量

- 専用スレッド1本。通常のサンプルでEPG全件を走査しない（EPG置換時に件数と文字列容量を集計）。
- ディスク書き込みとprocfsの読み取りは専用スレッドで実行。初回のディレクトリ・ファイル準備は起動時に行う。
- 待ちキューは32件。満杯なら操作を待たせず記録を省略し、累積省略数を `dropped_records` に記録する。
- 1ファイル最大4MiB、プロセスごとに現在と直前の2ファイル。
- 起動時に終了済みプロセスの古いログを整理し、最大6ファイルを残す。Linuxでは稼働中PIDのファイルを削除しない。同時起動が1つなら、整理直後の上限は計32MiB（終了済み24MiB＋現行8MiB）。同時起動数に応じて増える。
- 書き込みエラー時は警告して記録を停止する。終了直前や異常終了時には未書き込みの末尾記録が失われる場合がある。

比較時には、この記録機能自体による1スレッドとメモリの増加が含まれる点にも注意する。

## glibc・QML GCの切り分け

通常の状態記録には `allocator` を追加した。Linux/glibcでは `mallinfo2()` の
統計を記録し、それ以外の環境では `null` とする。glibc 2.33以降が必要。
旧ログにはこの項目がなく、集計では `unavailable` と表示する。

| 項目 | 内容（bytes） |
| --- | --- |
| `arena_bytes` | glibcのアリーナ容量 |
| `in_use_bytes` | glibcが使用中と集計する容量（uordblks） |
| `free_bytes` | アリーナ内の空き容量（fordblks） |
| `mmap_bytes` | mallocが直接mmapで確保した容量（hblkhd） |
| `releasable_top_bytes` | ヒープ末尾の返却候補容量（keepcost）。全空き領域ではない |

`mmap_regions`、`provider`、glibcの `version` も記録する。
`in_use_bytes` はアプリの生存オブジェクト容量と一致しない。tcacheや管理領域の影響があり、
この値の増加だけではリークと確定できない。QMLのJSヒープによる直接mmapやGPU割当は対象外。
アロケーターをLD_PRELOAD等で差し替えた環境ではglibc統計が全体を表さない。
統計取得時にglibc内部のロックを取るため、計測自体にも負荷がある。

`process` には `virtual_kib`（VmSize）、`anonymous_kib`（smaps_rollupのAnonymous）、
`lazy_free_kib`（LazyFree）も追加した。仮想サイズは実際の物理メモリー使用量ではない。
LazyFreeも全ての「解放済みメモリー」の合計ではない。procfsに項目がなければ `null`。

QtのGC前後の統計も必要な場合は、通常の視聴環境で次のように起動する。

```sh
MIRAKURUN_GC_LOG=1 ./build/mirakurun-viewer
```

`qt.qml.gc.statistics` と `qt.qml.gc.allocatorStats` のdebugを有効にし、
`kind: "qt_gc"` の行として同じ `usage-<PID>.jsonl` に保存する。
GCの発生時刻 `unix_ms`、カテゴリー、Qtのメッセージを保存する。
既存のQtメッセージ出力も維持する。明示的な `QT_LOGGING_RULES` / `QT_LOGGING_CONF` は
この設定より優先される。自分でこれらのカテゴリーを有効にした場合も保存される。

GC行は状態記録と同じ32件のキュー・4MiBのファイル上限を共有する。
1メッセージは最大4096 Unicode文字に制限し、満杯時は省略数に加算する。
このため大量のGCログで状態記録が省略されたり、保存期間が短くなる場合がある。
Playerの診断記録開始より前、および終了後のGCメッセージは保存しない。
GC行には状態スナップショットを付けず、前後の通常サンプルと時刻で照合する。
`MIRAKURUN_DIAGNOSTICS=0` はGCのファイル記録も無効にする。

起動後5分を除外して集計する例:

```sh
python3 scripts/summarize-usage.py ~/.local/state/mirakurun-viewer/usage --after 300
```

`endpoint_delta/min` は区間の最初と最後の差を経過分数で割ったMiB/分。
回帰分析やリーク判定ではなく、停止・選局が混ざるとその影響も含む。
GCメッセージ件数と省略数は、`--after` にかかわらず読み込んだファイル全体の値。
GCの数値はQtのバージョンで形式が変わり得るため、自動解析せず原文を保存する。

同じ操作を繰り返し、同じ状態へ戻った後の値を比較する。
`free_bytes` が増え、GC後の使用量が安定していれば未返却領域が候補になる。
GC後の使用量やネイティブ割当が増え続ける場合は、保持参照や割当元の追跡へ進む。
強制GC、`malloc_trim()`、`malloc_info()`のXML保存はこの記録機能では実行しない。

参考: [QtのJSメモリー管理](https://doc.qt.io/qt-6.10/qtqml-javascript-memory.html)、
[Qtのロギングルール](https://doc.qt.io/qt-6/qloggingcategory.html)、
[mallinfo2](https://man7.org/linux/man-pages/man3/mallinfo.3.html)。
