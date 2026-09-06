# glibcとRustのメモリー制御機構の調査

2026-09-06。調査環境はRust 1.98.1、Ubuntu glibc 2.43。
今回の実装は読み取り専用の `mallinfo2()` 診断まで。以下の本体組み込み案は未実装。

## 結論

環境変数以外に、Rustから `libc::mallopt()` を呼んでglibcの確保方針を設定できる。
今回のようにRust・Qt・GStreamerが混在するアプリでは、まずglibcの方針を調整する案が適切。
Rustのglobal allocator変更とは効果範囲が異なる。

| 制御 | 対象・目的 | 今回の位置づけ |
| --- | --- | --- |
| `MALLOC_MMAP_THRESHOLD_` / `GLIBC_TUNABLES` | プロセス起動時のglibc設定 | 環境変数での比較は実施済み |
| `mallopt(M_MMAP_THRESHOLD, n)` | 大きな確保のmmapしきい値を固定 | Rustから設定する第一候補 |
| `mallopt(M_TRIM_THRESHOLD, n)` | 返却可能なheap末尾の自動返却しきい値 | 必要が判明してから別に比較 |
| `mallopt(M_ARENA_MAX, n)` | arena数の上限 | 保持量と並列確保の競合を比較する必要あり |
| `glibc.malloc.tcache_count` | スレッドごとのキャッシュ量 | 今回の大きな領域保持への優先対策ではない |
| `malloc_trim(0)` | 現在の空きページの返却を試す | 停止時・メモリー圧迫時の補助候補 |
| `#[global_allocator]` | Rustの標準的な動的確保先を選択 | Qt/GStreamerのmallocを自動置換しない |
| `Vec`等の寿命・capacity管理 | アプリが所有するデータとバッファーの管理 | 引き続き必要。OS返却の制御とは別 |

glibcのパラメーターと効果は [mallopt公式説明](https://sourceware.org/glibc/manual/latest/html_node/Malloc-Tunable-Parameters.html)、
起動時設定とtcacheは [glibc tunables](https://sourceware.org/glibc/manual/latest/html_node/Memory-Allocation-Tunables.html) に基づく。

## Rustからglibcを設定する

既にこのブランチにはLinux/glibc限定で `libc` crateがある。例えば次のように扱える。

```rust
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn configure_allocator() -> Result<(), &'static str> {
    // 起動時、Qt/GStreamer/Tokioやアプリのスレッドを開始する前に一度だけ呼ぶ。
    let accepted = unsafe {
        libc::mallopt(libc::M_MMAP_THRESHOLD, 128 * 1024)
    };
    if accepted == 1 {
        Ok(())
    } else {
        Err("glibc rejected the allocator policy")
    }
}
```

これは以後のglibc malloc系の方針を変える。Rust専用の割り当てだけでなく、同じglibcを
使うネイティブライブラリー側も対象。ただしライブラリー独自のプール、直接mmap、GPUの
VRAMまで制御するわけではない。すでに確保・保持されたメモリーをこの呼び出しで返すわけでもない。

設定の呼び出し位置は `main` の起動計画決定直後、QML登録・QGuiApplication・再生preload前を候補とする。
main以前のランタイム／共有ライブラリー初期化もあるので、起動環境変数と完全に同じタイミングではない。
glibcマニュアルはmalloptに初期化・スレッド安全性の制約を示すため、再生中の設定変更にはしない。
返り値を確認し、設定できたふりをしない。

`std::env::set_var`をmainで呼ぶことは、すでに初期化済みのglibc設定を変えるAPIの代わりにはならない。
コードから制御するならmallopt、起動前から適用するならランチャー側の環境変数とする。

## 小さなRustプログラムでの動作確認

アプリと独立したプロセスで `System` をglobal allocatorとして明示し、malloptを設定。
RustのVecで2MiB、Cのmallocで2MiBを確保し、それぞれ正しいAPIで解放した。
表示・GPU・音声は使用していない。

```text
mallopt accepted
mmap bytes before:                 0
Rust Vec + C malloc:         4,202,496
after release:                     0
```

Rustからの設定がRust/SystemとC mallocの双方に作用することを確認したもの。
Qt/GStreamerを含む本体にmalloptを組み込んだ長時間試験ではない。
コード・結果は `benchmark/allocator-controls/mallopt.rs` と `result.log` に保存（Git対象外）。
以前の環境変数による実再生比較は [allocator-investigation.md](allocator-investigation.md) を参照。

## Rustのglobal allocatorとの違い

`#[global_allocator]`でjemallocやmimallocなどを選ぶことは可能。
標準的なBox・Vec等の確保経路が変わるが、C/C++が呼ぶmalloc/newはその属性では置換されない。
標準ライブラリー内部にもSystemへ直接確保する経路がある。
[std::alloc公式説明](https://doc.rust-lang.org/std/alloc/index.html)

Rustのdefault global allocatorは仕様上未規定で、すべてのRust環境がglibcを使う保証はない。
`System`はUnixではmalloc系、WindowsではHeapAlloc系を利用する。
またSystemの内部処理にはalignment調整があり得るため、Rustの確保物を勝手にC freeで解放しない。
[System公式説明](https://doc.rust-lang.org/std/alloc/struct.System.html)

ネイティブ側も含めて別allocatorへ置換するなら、Rust属性とは別に、リンクや
`LD_PRELOAD`によるプロセスのmalloc置換を検討することになる。
例えば [mimalloc公式資料](https://github.com/microsoft/mimalloc#dynamic-override) がこの方式を案内している。
ライブラリーの組み合わせと確保・解放の対応を改めて検証する必要があるため、今回の第一候補にはしない。

## 解放の追加制御

`malloc_trim(0)`はglibcが持つ空きページをOSへ返そうとするGNU API。
使用中オブジェクトを解放するGCではなく、すべての空き領域が返る保証もない。
0は「返せなかった」であり、エラーとは限らない。現行glibcでは全arenaのページ単位の空きを対象にする。
[Linux man-pages](https://man7.org/linux/man-pages/man3/malloc_trim.3.html)

判断としては、まずmmapしきい値固定だけを比較し、trimを同時に毎フレーム・毎pollへ追加しない。
複数の対策を重ねると効果の帰属が曖昧になり、返却後の再確保の負荷も評価しにくくなる。

Rust側の `Vec::clear()` は要素を削除するがVec自身のcapacityは残す。
`shrink_to_fit()` は容量を減らす要求で、allocatorが余分な容量を残すこともある。
これらを呼んでもOSへのページ返却まで保証するものではない。
[Vec公式説明](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.shrink_to_fit)

## このリポジトリへの提案

起動計画に「標準方針」と「glibc mmapしきい値128KiB固定」の明示的な選択を追加し、
Linux/glibcの場合だけ `memory` モジュールで初期設定する形が小さく実装できる。
明示的なアプリ設定を選んだ場合だけmalloptを呼ぶ案なら、標準方針では既存の環境指定を維持できる。
設定は再生・字幕・EPGの内部には置かず、プロセスの起動責務にまとめる。

採用判断は、設定固定での長時間視聴、停止再開、選局、EPG更新、字幕描画を通し、
RSSだけでなくCPU負荷・フレーム落ち・開始時間も比較して行う。
今回の調査でアプリの通常の確保方針や稼働中プロセスの設定は変更していない。
