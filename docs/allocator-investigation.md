# EPG有効時の停止・再開とRSS増加（2026-09-06）

番組表を一度も開かず、字幕OFF・EPG ONの状態で停止・再開すると、1回あたり
20〜30MBほど増えるという報告を受けて実測した。

## 同じ操作での比較

各条件を新規プロセスで実施。NHK総合1・京都を再生し、初回12秒待機、
停止0.7秒→再生3秒を12回、最後に11秒待機。番組表・字幕は全条件で無効。
EPG ONの場合は初回に13,583番組を取得。5分更新を迎える前に試験終了。
既存の視聴プロセスはそのまま残しており、専有環境の性能測定ではない。

| 条件 | 初回再生RSS | 12回後・待機後RSS | 最後に停止した後RSS |
| --- | ---: | ---: | ---: |
| EPG ON・標準設定 | 244.8 MiB | 530.5 MiB | 530.3 MiB |
| EPG OFF・標準設定 | 240.6 MiB | 269.6 MiB | 250.2 MiB |
| EPG ON・mmapしきい値128KiB固定 | 236.6 MiB | 249.9 MiB | 226.6 MiB |

3条件とも初回＋再開12回のPLAYINGログ13件を確認。エラー・クラッシュなし。
設定固定以外の再生・EPG処理を変更していない。

## glibcの使用中／空き領域

`mallinfo2()`の読み取りを10秒ごとの診断へ追加した。allocatorを変更したり
`malloc_trim`を呼んだりする計測ではない。

EPG ON・標準設定の初回再生と12回後の比較：

| glibcの集計 | 初回 | 12回後 |
| --- | ---: | ---: |
| arena使用中＋直接mmap確保 | 174.5 MiB | 175.2 MiB |
| arena内の空き領域 | 44.9 MiB | 351.9 MiB |

RSSの約286MiB増に対し、glibc使用中の増加は約0.7MiBで、空き領域が約307MiB増えた。
今回の大きな増加について、解放済み領域の保持が主要因であることを強く示す。
Qt文字描画キャッシュや、EPG13,583件のコピー蓄積だけで説明する結果ではない。

## 設定固定の意味と限界

比較実験では、新しいプロセスだけに次を設定した：

```sh
MALLOC_MMAP_THRESHOLD_=131072 QT_QPA_PLATFORM=xcb \
MIRAKURUN_SERVER=http://192.168.3.3:40772 \
./build/mirakurun-viewer --features=epg
```

[glibcの仕様](https://sourceware.org/glibc/manual/latest/html_node/Memory-Allocation-Tunables.html)
では、大きな割り当てにmmapを使うしきい値は通常動的に調整され、明示指定すると固定になる。
EPG取得時の一時的な大きな確保などが、その後の再生バッファーの扱いに影響する可能性がある。
どの割り当てで動的しきい値が変わったか、しきい値の実値までは追跡していない。

`mallinfo2`はRSSそのものではなく、allocatorの論理的な使用中／空き領域の集計。
すべてのGPU・ドライバー・直接mmap資源や、正味の有効データ量を表すものではない。
この結果をアプリ全体にリークが一切ない証明として扱わない。

しきい値固定は今回の比較実験で、通常起動の設定は維持する。恒常採用に向けた
長時間再生、CPU負荷、フレーム落ち、別の映像形式での比較は未実施。

## 保存物

- `rust/src/memory.rs`：Linux/glibc向けの読み取り専用診断。`ALLOC`行にUnix時刻とKiB値。
- `benchmark/allocator-comparison/`：標準設定の2条件、操作別RSS、使用中／空き領域、テストログ。
- `benchmark/allocator-fixed/`：しきい値固定時のログ。
- `benchmark/allocator-comparison/fixed-results.jsonl`：しきい値固定時の操作別RSS。

benchmarkはGit対象外。自動テスト28件成功、外部TSを必要とする任意テスト1件は未実行。
