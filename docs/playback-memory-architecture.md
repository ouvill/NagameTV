# 起動・再生時のメモリーと処理負荷

2026-09-23に、起動直後の常駐量と再生中の使用量を調べた。`Text.NativeRendering`から
`Text.QtRendering`への変更で継続的な増加が改善したという報告を前提に、今回は
起動時の確保、デコーダーの並列数、CPU処理とGPU転送の配置を対象とした。

## 採用した構成

CPU経路では、デインターレースを済ませてから表示形式へ変換し、8フレームを先読みする。
従来は先にI420をNV12へ変換していたため、後段のデインターレースと待ち行列も
`glupload`が提案する転送用プールを利用していた。順序を変えることで、デインターレースの
参照画像をI420のまま保持し、NV12への変換後は転送用プールで再利用できる。
入力側にも`videoconvert`を残し、デインターレースが直接扱えない形式には変換を挟む。
I420入力ではここをpassthroughする。この前後の変換配置は
[GStreamerのdeinterlace使用例](https://gstreamer.freedesktop.org/documentation/deinterlace/index.html)にも沿っている。

```mermaid
flowchart LR
    TS[容量制限付きTS保持] --> Decode["デコード<br/>ソフトウェア並列数に上限"]
    Decode --> Deinterlace[デインターレース]
    Deinterlace --> Convert[表示形式へ変換]
    Convert --> Queue[先読み8フレーム]
    Queue --> Upload[GL転送・プール再利用]
    Upload --> Qt[Qtと共有するGL表示]
```

この分離はI420→NV12のように変換が必要な場合に成立する。入力が既に表示形式と
同じならpassthroughできる。GPU経路は既存のGLMemory／VAMemory契約を維持する。
GLコンテキストはQtのものをGStreamerへ渡す。別のコンテキストを後から作ると、
バッファーの再作成やGPU→CPU→GPU転送を招くため、`qml6glsink`を先にREADYへ遷移させる
[公式の初期化順序](https://gstreamer.freedesktop.org/documentation/qml6/qml6glsink.html)を保った。

映像キューは8枚のままにした。4枚へ減らした後に引っ掛かりが報告され、8枚へ戻した
履歴があるため、今回の削減を先読みの短縮で実現しない。満杯時は上流を待たせ、
フレームを捨てない。利用していないキュー通知は`silent=true`で抑制した。
[GStreamerのqueue仕様](https://gstreamer.freedesktop.org/documentation/coreelements/queue.html)

libavの映像デコーダーには、利用可能なCPU数を超えない範囲で通常6、HEVCでは10の
スレッド上限を設定する。`playbin3`の`element-setup`で毎回適用し、録画切り替えで
新しいデコーダーが生成されても設定を維持する。音声・GPUデコーダーの設定には介入しない。
映像診断JSONの`decoder_thread_limits`と起動ログで適用値を確認できる。この値は
`max-threads`の設定値であり、実際に稼働中のワーカー数ではない。

設定画面は最初の表示要求で生成する。再表示の速さと編集中の値を保つため、初回表示後は
保持する。設定・接続・タイムシフトなどの確定した状態と処理は従来どおりRustが所有する。
設定画面を開かない視聴では、その画面のQObject、バインディング、JavaScriptオブジェクトを
確保しない。[Qt Quickの生成と寿命に関する説明](https://doc.qt.io/qt-6/qtquick-performance.html)

## 参照した実装と判断

| 参照先 | 確認した実装 | このアプリでの判断 |
|---|---|---|
| [TVTest CoreEngine](https://github.com/DBCTRADO/TVTest/blob/dce8b3e5e19cb6b1b12c60ff839b943a0685d715/src/CoreEngine.cpp) | TSの解析・EPG・字幕・再生をフィルターとして接続し、視聴用バッファーの容量と初期プール率を設定する | TSの保持、解析、映像出力の所有を分ける既存構成を維持する |
| [BonDriver_Mirakurun](https://github.com/Chinachu/BonDriver_Mirakurun/blob/247d8e91601484157055c8811e8cd59c441cf67c/src/tuner.rs) | 受信TSのキューに上限を設け、満杯時は古いチャンクを除去する | 現在のTS保持も容量制限付き。履歴OFF時は2 MiB／2秒の範囲。上限だけを理由に再実装しない |
| [NicoJK CommentWindow](https://github.com/xtne6f/NicoJK/blob/7497c26e3777b86ec61efb6a14054d8ccd83d215/CommentWindow.cpp) | 表示期限切れのコメントを削除し、描画用テクスチャーを再利用・整理する | 現在の実況64件・履歴200件の上限とQtRenderingを維持する。起動時の描画基盤の負担とは分けて評価する |
| [VLC 3.0系のlibav接続](https://github.com/videolan/vlc/blob/3.0.x/modules/codec/avcodec/video.c) | 自動並列数を通常6、HEVCでは10に制限し、フレーム並列数に応じて画像バッファーも確保する | 同じ上限を参考に、利用可能CPU数も考慮したデコーダー方針を追加した。VLCのMPEG-2用thread-type変更までは移植していない |
| [GStreamer libav](https://gstreamer.freedesktop.org/documentation/libav/avdec_h264.html) | `max-threads=0`は自動選択 | Qtの描画、デインターレース、音声とCPUを分け合うため、映像復号の自動並列数を制限する |
| [GStreamer Bufferpool](https://gstreamer.freedesktop.org/documentation/additional/design/bufferpool.html) | ALLOCATION queryで下流のプールを上流へ提案し、参照が外れたバッファーを再利用する | デインターレースの後へ形式変換を移す。プール再利用は維持する |

参考にしたBonDriver_Mirakurunは閲覧時の既定ブランチ`feature/refactor-rust`。
TVTest、NicoJK、BonDriver_Mirakurunの実装を別OS上のメモリー量と直接比較してはいない。

`identity drop-allocation=true`でALLOCATION queryを止める方法は採用していない。
[GStreamer 1.28のraw upload](https://github.com/GStreamer/gstreamer/blob/1.28/subprojects/gst-plugins-base/gst-libs/gst/gl/gstglupload.c)は
毎フレームのテクスチャー生成を残しており、転送プールを使わなくするだけでは確保・解放の
回数を増やし得る。アロケーターの変更や定期的な`malloc_trim`も今回は追加していない。

## 計測方法

Linuxの専用Weston/Xwayland環境、AMD Ryzen 7 3700X（16論理CPU）、
NVIDIA GeForce RTX 4070 Ti、ドライバー595.84、
Qt 6.10.2、GStreamer 1.28.2で測った。公開GUIランチャーで実GPUとPipeWireの
専用仮想出力を検証した。ホストのデスクトップや物理音声出力は使用していない。

Releaseビルドを変更前後でコピーし、同じ入力・ウィンドウサイズ・診断設定で別プロセスとして
起動する。各プロセスに空の設定・キャッシュディレクトリーを割り当て、ローカルHTTPサーバーが
PCRに合わせてTSを配信する。番組一覧は空、実況OFF、タイムシフトOFF、音声はPulseAudio時計。
接続先は環境変数で指定し、初回案内画面は表示しない。
これは再現可能な映像・音声の比較であり、実局の大量EPGや実況を重ねた測定ではない。

1秒ごとに`/proc/PID/smaps_rollup`とCPU時間を記録し、起動後10秒以降のメモリー中央値を
採用する。RSSは共有ページを含み、PSSは共有分を按分し、USSは専有ページだけを数える。
CPUは1論理CPUの占有を100%とする。描画数・破棄数はGStreamer sinkの診断値であり、
物理ディスプレイへの提示時刻や音声デバイスの遅延を測るものではない。
ウィンドウは1280×720。MPEG-2は1920×1080i/30fpsをYADIFで60fpsにし、
H.264は1920×1080p/30fpsをそのまま表示する。どちらもAAC音声を含む。

### 変更前後の結果

各条件を35秒ずつ3回起動し、各回の安定区間の中央値から、さらに中央値を取った。
変更前は`3af3989725adc0218429701cf49feb38f069d9c8`のReleaseビルド。
メモリーの単位はMiB。

| 条件 | RSS | PSS | USS | CPU % | プロセス全体のスレッド数 |
|---|---:|---:|---:|---:|---:|
| 変更前・未再生 | 225.6 | 185.4 | 174.6 | 0.36 | 13 |
| 変更後・未再生 | 220.5 | 180.4 | 169.8 | 0.32 | 13 |
| 変更前・MPEG-2 | 332.7 | 291.2 | 279.6 | 72.32 | 40 |
| 変更後・MPEG-2 | 331.6 | 290.1 | 278.5 | 48.08 | 30 |
| 変更前・H.264（自動選択） | 373.3 | 325.8 | 311.5 | 11.52 | 30 |
| 変更後・H.264（自動選択） | 365.9 | 318.4 | 304.1 | 11.24 | 30 |
| 変更前・H.264（libav固定） | 417.7 | 376.4 | 364.9 | 15.40 | 41 |
| 変更後・H.264（libav固定） | 342.9 | 301.5 | 290.0 | 15.96 | 31 |

未再生時のRSSは約5.1 MiB減った。MPEG-2再生時のRSSは変更前329.0〜335.2 MiB、
変更後329.8〜333.4 MiBで、差は測定のばらつきの範囲に収まる。
この条件で再生メモリーが大幅に減ったとは判断していない。
CPU使用率の中央値は約34%下がり、両構成とも安定区間は約60fps、破棄0フレームだった。

H.264の自動選択では、両構成のマッピングにNVDEC/CUDAライブラリーが含まれていた。
この環境では[GPUデコーダーの優先度](https://gstreamer.freedesktop.org/documentation/nvcodec/nvh264dec.html)が
libavより高く、ソフトウェア向けスレッド上限の評価にはならない。
約30fps・破棄0フレームを維持し、RSSの差は約7.4 MiBだった。

H.264を`GST_PLUGIN_FEATURE_RANK=avdec_h264:512`でlibavに固定すると、
RSSは約74.8 MiB（約18%）減った。変更前は安定区間17.8〜22.3fps・破棄1〜3フレーム、
変更後は全3回とも約30fps・破棄0フレームだった。変更前は同じ枚数を描画できていないため、
このCPU値を単純な処理効率の比較には使わない。映像ワーカー数を制限する方針は、
この環境ではメモリー量と再生の継続性の両方を改善した。

### VLCとの比較

VLC 3.0.23（Qt Widgets 5系の画面）も同じ専用環境で測った。
同じMPEG-2入力をHTTPで渡し、`--avcodec-hw=none --vout=gl --aout=pulse`
と`--deinterlace=1 --deinterlace-mode=yadif2x`を指定した。
PulseAudioの専用sinkへの接続と音声バッファーの再生、GL表示、YADIF使用をログで確認した。
通常のVLCの既定設定や、ユーザーのデスクトップ環境を再現する測定ではない。

| VLCの条件 | RSS MiB | PSS MiB | USS MiB | CPU % |
|---|---:|---:|---:|---:|
| 未再生 | 118.3 | 81.3 | 71.2 | 0.00 |
| MPEG-2・参考値（下記制約あり） | 370.5 | 306.5 | 285.4 | 20.08 |

VLCは未再生時の使用量が小さい。一方、この条件の再生ではVLC側に継続的な遅延・
フレーム破棄があり、RCの`frames displayed`は39.5〜39.8fpsにとどまった。
10〜30秒の区間に203〜206フレームが破棄された。
同じ描画量に達していないため、VLCのCPU値を本アプリの約60fpsと比べて優劣を判断しない。
再生時の値から「VLCより軽くなった」とも結論しない。ユーザー環境での差を詰めるには、
VLCのデコーダー、表示方式、デインターレース設定と入力映像を揃えた追試が必要になる。
起動時はQt QuickのGL初期化を含む構成上の差が残っている。

測定の生データは作業ディレクトリーの`benchmark/playback-memory-20260923/`に保存した。
変更前のMPEG-2／未再生は`final-mpeg2/binary-1-*`、変更後は`final-measurements`、
H.264は`h264-comparison`（自動選択）と`h264-software`（libav固定）にある。
VLCの有効な測定は`vlc-validated`。それ以前のVLC試行は起動オプションやライブラリーの
検索パスが不備だったため、比較には使っていない。VLC本体と依存ライブラリーは
作業用ディレクトリーへ展開し、システムへのインストールやコンテナー設定の変更は行っていない。
`final-mpeg2/binary-2-*`は途中の構成であり、
上表の変更後の値には使っていない。`metadata.json`にビルド情報・入力ハッシュを保存し、
最終計測にはバイナリーのハッシュも含めた。35秒の比較は起動後の常駐量を調べるもので、
長時間の増加や4K・HEVC・実局の高ビットレート映像の速度は評価していない。

### 確保元

順序変更前の再生をheaptrackで別に測定したところ、ピーク時の確保元には
NVIDIA GLドライバー128.27 MB、`gst_gl_base_memory_alloc_data`49.77 MBが含まれた。
後者は32回の確保で、1080p NV12の16枚分の画素量に相当する。通常実行のRSSは
heaptrackのRSSと混ぜて比較しない。これらはmallocで追跡できた確保量であり、VRAM使用量
全体ではない。上位の関数別ピークを単純に合計してプロセスRSSとも比較しない。

起動時の`smaps`には、`libnvidia-gpucomp`約29.6 MiB、`libnvidia-glcore`約14.7 MiB、
デバイスマッピング約18.5 MiBが常駐していた。Qt Quickは再生前からOpenGLを使うため、
Rustのデータ構造や未使用画面だけを削減しても、この負担は残る。
Rust専用のヒープ計測クレートではQt/GStreamer/ドライバーのmallocを網羅できないため、
新しい通常依存は追加せず、`/proc`・既存の診断記録・heaptrackを使った。

## 再現する

ビルド済みの変更前バイナリーを別名で保存し、変更後を同じ設定でReleaseビルドする。
以下の`before-nagametv`はその保存先。出力先は既存でないディレクトリーを指定する。

```sh
python3 scripts/benchmark-playback-memory.py benchmark/memory-comparison \
  --binary /path/to/before-nagametv --binary build/nagametv \
  --ts /path/to/fixture.ts --service-id 1 --repeat 3 --seconds 35
```

H.264のソフトウェア復号を比較するときは、同じコマンドへ
`GST_PLUGIN_FEATURE_RANK=avdec_h264:512`を付けて起動する。
指定しない場合はGStreamerの既定のデコーダー選択を測る。

TSは188バイト単位で、PCRが連続し、測定時間より長いものを使う。複数番組やPCRの
リセットを含むTSはこの速度比較用のfixtureには使わない。`--phase idle`、
`--phase playback`で片方を選べる。`--deinterlace gl`などで処理経路を明示できるが、
失敗時にCPU経路へ切り替えることはない。この比較スクリプトはXwaylandのウィンドウを
操作するため、Waylandを必要とするVA-API経路は対象外。

出力は各起動の`samples.json`、`summary.json`、`smaps.txt`、アプリログ、診断JSONL。
`metadata.json`にはビルド情報、バイナリーとTSのSHA-256、測定条件を保存する。
ウィンドウの検出時刻は1秒刻みのポーリング値で、正確な初回フレーム時刻ではない。
ベンチマーク中はビルド・GUI試験・別プレイヤーを並行実行しない。

同じMPEG-2入力を作る例。明示したCPUエンコーダーだけを使い、機器は不要。

```sh
gst-launch-1.0 -q \
  videotestsrc num-buffers=4200 pattern=ball ! \
  video/x-raw,format=I420,width=1920,height=1080,framerate=60/1 ! \
  interlace field-pattern=1:1 top-field-first=true ! \
  avenc_mpeg2video flags=ildct+ilme bitrate=8000000 ! mpegvideoparse ! queue ! \
  mpegtsmux name=mux ! filesink location=fixture.ts \
  audiotestsrc num-buffers=3282 samplesperbuffer=1024 ! \
  audio/x-raw,rate=48000,channels=2 ! audioconvert ! avenc_aac ! aacparse ! queue ! mux.
```

## 実施した検証

Releaseビルドに加え、次の検査が成功した。

- ハードウェア不要のRustテスト：303件成功、既存の5件はignore。
- 通常構成・`native_tests`構成のClippy（警告をエラー扱い）。
- QML全88ファイルのlint、UIスタイル検査、Rustの整形検査、計測スクリプトの構文・起動確認。
- 接続・モデル通知の結合試験、デスクトップメディア連携試験。
- 製品`Main.qml`の全起動試験：初回・設定済み起動、自動再生と環境変数の優先関係、
  設定画面・番組表、録画の時計／PID変更、タイムシフトの容量制限と停止。
- 映像処理試験：YADIFのNV12／RGBA、OFFのソフトウェア／NVDEC、NVDEC＋OpenGL。
  各経路でインターレース／プログレッシブの切り替え、再開、caps、フレームレート、画像取得を確認。
  NVDEC＋OFFでは入力・表示ともGLMemory NV12を維持した。
- キャプチャー試験：表示中の番号付きフレーム、1080p・4:3・PAR、字幕・コメント合成、
  リサイズ・全画面・非表示状態・連写・停止。

GUI試験は公開スクリプトで専用画面・実GPU・専用音声出力を検証してから実行した。
最終実行ログは`build/memory-*-final.log`。VA-APIと他GPUは今回の検証対象に含めていない。
