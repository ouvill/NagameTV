# GPU映像処理とNV12表示

`NAGAMETV_DEINTERLACE`で起動時に経路を選択する。既定の`yadif`は維持する。
GPUモードは明示指定とし、必要なデコーダー・処理方式・メモリー形式が使えない場合は
エラーにする。途中でCPUデコード／デインターレースに切り替えない。

| 指定 | デコード・処理・表示 | 1080i入力の出力レート |
|---|---|---|
| `yadif` | 自動選択デコーダー → CPU YADIF → GL upload → NV12表示 | フィールドごと（29.97 → 59.94 fps） |
| `linear` | 自動選択デコーダー → CPU linear → GL upload → NV12表示 | フィールドごと |
| `off` | 自動選択デコーダー → GL import/upload → NV12表示 | 入力と同じ |
| `gl` | NVIDIA NVDEC → GLMemory NV12 → GPUでRGBA変換 → OpenGL vfir → RGBA表示（NV12も明示指定可） | 入力と同じ（29.97 fps） |
| `va`（Linux/Wayland） | VAデコーダー → VAMemory NV12 → vadeinterlace adaptive → vapostproc → DMABuf → GL import → NV12表示 | フィールドごと |

`gl`はNVIDIAのNVDECを利用する経路。既存の`gldeinterlace`はRGBA専用で、
フィールド倍速出力に対応しないため、YADIFと同等品質・同等の動きの滑らかさではない。
vfirは上下の走査線を使う垂直フィルターで、細部の解像感も変わる。
プログレッシブ入力はフィルターをバイパスする。GPU内のNV12→RGBA変換と
NVDEC→OpenGLのGPU内コピーは残るが、フレームのCPU往復を要求しない。
`NAGAMETV_VIDEO_FORMAT=nv12`を明示すると、処理後にGPUでNV12へ再変換して表示する。
この場合もCPU往復は要求しないが、中間RGBAは残り、GPUの変換パスと出力面が増える。
転送量・速度の改善を保証する指定ではないため、`auto`ではこの追加変換を省く。
GStreamer 1.28.2のgreedyhには停止後も前フレームのtextureポインターを保持する実装が
あるため、再生対象を切り替える本アプリでは前フレームを参照しないvfirを使う。

`va`はLinuxのVA-API対応GPU／ドライバー用。NV12デコード、adaptive方式、
DMA_DRM形式のDMABuf出力、Qtと同じGPUでのEGL importが必要。
現在のqml6glsink（1.28.2を含む）はX11上のQtコンテキストをGLXと仮定するため、
このVA経路はWaylandを必要とする。`va`指定時は未指定の`QT_QPA_PLATFORM=wayland`を
Qt起動前に設定し、X11を明示した場合は説明付きエラーにする。
`va`指定時は未指定の`GST_GL_PLATFORM=egl`も設定する。
明示した環境設定は保持し、互換性がなければ再生エラーになる。GPUやドライバーによって
利用できるコーデック・フィルターが異なる。NVIDIA向けの`nvidia-vaapi-driver`で
デコードできることだけでは、VAデインターレースも使えるとは判断しない。
adaptive方式はドライバーが公開する`method`のenumを検証してから設定する。
非対応なら説明付きエラーにし、プロパティー文字列の変換によるpanicを避ける。

## 起動例

```sh
NAGAMETV_DEINTERLACE=gl ./build/nagametv
NAGAMETV_DEINTERLACE=gl NAGAMETV_VIDEO_FORMAT=nv12 ./build/nagametv
NAGAMETV_DEINTERLACE=va NAGAMETV_VIDEO_FORMAT=nv12 ./build/nagametv
NAGAMETV_DEINTERLACE=yadif NAGAMETV_VIDEO_FORMAT=nv12 ./build/nagametv
```

`NAGAMETV_VIDEO_FORMAT=auto|nv12|rgba`でsinkの形式を選べる。既定の`auto`は
NV12とRGBAを許可し、入力に応じて交渉する（`gl`はRGBA）。CPU処理後はNV12へ
変換してから転送し、GLMemoryのNV12入力はそのまま表示する。10bit AV1／HEVCなどの
P010 GL入力は、NV12への直接変換に対応しないためGPU上でRGBAへ変換する。
古いpluginでNV12を扱えない場合もRGBAを選ぶ。`nv12`を明示した場合は
未対応pluginではエラーにする。GStreamer 1.24との互換性は維持する。
これは色形式の互換処理であり、GPU故障時のソフトウェア描画への切り替えではない。

GPUモードのデコーダー選択には、このアプリのプロセス内だけで映像デコーダーの
GStreamer rankを制限する。playbin3にはインスタンス単位のデコーダー指定APIがないためで、
音声デコーダーには影響しない。CPUモードは既存の`GST_PLUGIN_FEATURE_RANK`設定を尊重する。
設定は起動単位で、同じプロセスでの処理方式の切り替えは扱わない。

## 転送と表示

CPU経路はデインターレース→形式変換→先読み→GL転送の順に処理する。
I420→NV12変換が必要な入力では、デインターレースを済ませてから転送用プールへ書き込む。
libav映像デコーダーの並列数にも上限を設ける。
[設計理由と測定条件](playback-memory-architecture.md)を参照。

- GPUモードは入力をGLMemoryまたはVAMemoryに限定し、VA出力もDMA_DRMのDMABufに限定する。
  CPUメモリーを経由する暗黙の代替経路を許可しない。
  VA経路ではglupload直後をNV12／RGBAの2Dテクスチャに限定する。sinkの形式だけを
  制限すると、RGBA指定でも途中にYUY2など別形式の取り込みが選ばれる場合がある。
- `off`でもNV12表示を使える。videoconvertはCPUのI420→NV12変換が必要な場合に働き、
  GLMemory NV12などのGPU出力にはpassthroughする。要素の存在だけをCPU往復の証拠とはしない。
- `yadif`と`linear`ではCPU処理が必要。videoconvertは対応する入力ならpassthroughし、
  GL側も既にNV12なら色変換をpassthroughする。GPUデコードを選んだ場合は
  CPUデインターレース前のダウンロードが残る。
- qml6glsinkを先にREADYにしてQtのGL display/contextを共有する既存の順序を維持する。
  sinkがまだplaybinに組み込まれていないREADY段階でも共有できるよう、sinkから取得した
  `gst.gl.GLDisplay`をplaybinへ明示的に渡す。VA経路でもデコーダーの起動前に
  vadeinterlaceをREADYにし、その`gst.va.display.handle`をplaybinへ渡す。
  再生対象の切り替えで出力binが一時的に外れても、次のデコーダーと既存のVAフィルターが
  同じdisplayを使う。`enable-last-sample=false`を維持し、
  映像queueの上限は8フレーム。GPU経路の導入時には保持する画像数を減らすため
  8から4へ変更したが、2026-09-20に継続再生中の小さな引っ掛かりが報告されたため、
  短い上流処理の遅れを吸収する余裕として8へ戻した。保持メモリーは増える。
  今回の症状がキュー不足によるものか、増量で改善するかは実視聴での比較が必要。
  通常の先読みでもキューは満杯になり得る。キューの位置は経路ごとに異なるため、
  同じフレーム数でも保持時間が同じとは限らず、この値だけで表示遅延全体は分からない。
- NV12の画素量は8-bit RGBAの3/8（約62.5%減）。実際のVRAM使用量にはstride、
  texture配置、デコーダーの参照面が加わるので、この比率をアプリ全体の削減率とは扱わない。
- スクリーンショットは表示されたGPUフレームを保持し、保存ワーカーでだけダウンロードする。
  NV12からRGBAへの変換はGStreamerのVideoConverterで色域・レンジを反映する。
  字幕・コメント・PAR補正も従来どおり保存する。通常再生中に画素のCPU読戻しは追加しない。

動画統計パネルの画素形式欄にメモリー形式、再生エンジン欄にデコーダー名も表示する。
動画統計JSONの`decoders`に実際の映像デコーダー名、`input.memory`と`output.memory`に
ネゴシエーションされたメモリー形式を出す。`processor_passthrough`でOpenGLフィルターの
省略状態も確認できる（インターレースはfalse、プログレッシブはtrue）。これらと出力fps／画素形式を合わせて確認する。
GLMemoryはCPU往復不要な経路の確認には使えるが、ドライバー内部のコピー回数を証明するものではない。

配布パッケージは、同梱qml6glsinkのNV12／Wayland対応とGPUドライバーの公開状態も必要。
特に現在のAppImage／Flatpakのqml6ビルドはWaylandを明示有効化していないため、
`va`経路の対象はまずネイティブ版とする。NV12非対応の旧pluginは`auto`でRGBAを使う。
AppImageにはNVDECとVAのpluginライブラリーを同梱するが、ドライバーはホスト側に必要。

## 検証

2026-09-20には専用Weston／NVIDIAでnative Waylandも確認した。Qtの自動選択で
Waylandプラグインを読み込み、yadif＋NV12とNVDEC＋OpenGL＋NV12の再生・映像切り替え・
キャプチャ試験が通過した。字幕・コメントを含むキャプチャもWaylandで確認した。
Waylandの起動・入力試験全体は専用環境のサイズ・フォーカス制約により未完了。
[表示方式の選択と検証範囲](platform-startup.md)を参照。

ハードウェア不要のRust試験は形式選択、CPUのフィールド倍速出力、NV12のstride・レンジ変換、
表示フレーム保持を検証する。GPU試験は[専用GUI環境](gui-test-environment.md)で実行する。

```sh
NAGAMETV_DEINTERLACE=yadif NAGAMETV_VIDEO_FORMAT=nv12 bash scripts/test-startup.sh video-processing
NAGAMETV_DEINTERLACE=gl bash scripts/test-startup.sh video-processing
NAGAMETV_DEINTERLACE=gl NAGAMETV_VIDEO_FORMAT=nv12 bash scripts/test-startup.sh video-processing
NAGAMETV_DEINTERLACE=va NAGAMETV_VIDEO_FORMAT=nv12 bash scripts/test-startup.sh video-processing
NAGAMETV_DEINTERLACE=va NAGAMETV_VIDEO_FORMAT=rgba bash scripts/test-startup.sh video-processing
NAGAMETV_VIDEO_FORMAT=nv12 bash scripts/test-startup.sh screenshot-playback
```

`video-processing`はCPUで1080i/1080pのMPEG-2を生成し、製品Main.qmlで再生して、
デコーダー、メモリー形式、出力fps、保存画像の寸法・空画像でないこと、映像切り替えを検証する。
同じ形式での再開、インターレース→プログレッシブ→インターレースの切り替えを含む。
追加のfixture生成pluginとして`interlace`と`avenc_mpeg2video`を使用する。
物理画面やスピーカーの試験ではなく、全コーデックや全フィールド順序の画質保証でもない。
VA-APIの実機検証には対応GPUが必要で、NVIDIAの試験結果で代用しない。

## 検証環境と範囲（2026-09-20）

Ubuntuのネイティブ版、GStreamer 1.28.2、NVIDIA GeForce RTX 4070 Ti / driver 595.84で、
専用GUI環境を検出・検証して実行した。1080i/1080pのMPEG-2を切り替えて確認した結果:

| 経路 | 実デコーダー | 表示形式・メモリー | 25fpsのインターレース入力 |
|---|---|---|---|
| yadif | avdec_mpeg2video | NV12 / GLMemory | 50fps |
| gl / auto | nvmpeg2videodec | RGBA / GLMemory | 25fps・フィルター実行 |
| gl / nv12 | nvmpeg2videodec | NV12 / GLMemory（中間RGBA） | 25fps・フィルター実行 |
| off / nv12 | avdec_mpeg2video | NV12 / GLMemory | 25fps |
| off / nv12 + NVDECのrank優先 | nvmpeg2videodec | 入力・表示ともNV12 / GLMemory | 25fps |

GPUフィルターの省略状態はインターレースでfalse、プログレッシブでtrueとなることも
製品のパイプラインから取得して検証した。NV12のスクリーンショットでは表示フレームの一致、
字幕・コメント合成、PAR、連写、リサイズ、停止を確認した。

VA-APIのデインターレース対応GPU／ドライバーがこの環境にないため、`va`の実機試験は未実施。
AppImage／Flatpakの配布物、他GPU、全コーデック、フィールド順序ごとの画質は未検証。
この時点ではCPU使用率・転送帯域・消費電力の改善率は計測していない。特にglは出力fps・方式が異なるので、
yadifとの単純な負荷比較を同等画質・同等処理量の速度比較には使えない。

## VA-APIの再生切り替え修正（2026-09-21）

Fedora 44、GStreamer 1.28.7、Qt 6.11.2、AMD Radeon Graphics（Renoir）／
Mesa 26.2.2の専用Weston環境で、MPEG-2の1080i再生後に1080pへ切り替えると
表示が止まる問題を再現した。次のデコーダーが別のVA displayを生成し、
既存フィルターに`Can't replace VA display while operating`、続いて
`vaBeginPicture: invalid VASurfaceID`が発生していた。

再生開始前にvadeinterlaceをREADYへ遷移させ、context queryで取得した
VA displayをplaybinへ設定する。VA要素が自分で生成したdisplayは通常の
GstElementのcontext一覧からは取得できない。これにより、再生対象の切り替え前後で
デコーダー・フィルター・allocatorのdisplayを共有する。開始途中で失敗した場合も、
先にREADYにした処理要素を終了時に明示的にNULLへ戻す。

RGBA表示では、vapostprocの出力にYUYVが選ばれ、AMDドライバーが
`vaEndPicture: operation failed`を返す問題も確認した。glupload直後をNV12／RGBAの
2Dテクスチャに限定する。表示先だけを制限しても、途中のYUY2をglcolorconvertで
変換できるため、対応する画素形式の制約は取り込み直後に必要となる。

修正後、MPEG-2の1080i→1080i→1080p→1080p→1080iの再生・停止・再開を確認した。
デコーダーは`vampeg2dec`、入力はVAMemory NV12、表示はGLMemory NV12／RGBAで、
25fpsのインターレース入力は50fps、プログレッシブ入力は25fpsとなった。
各再生で一時停止とキャプチャを行い、1920×1080の画像と空画像でないことも検証した。
同じ試験をyadif＋NV12でも実行し、全3経路・計15回の再生が成功した。
機器不要のRustテストは276件成功・5件ignoredで、adaptive非対応enumのエラーも確認した。
AMD環境ではDMABuf段階でRGB形式へ変換される場合があり、NV12を全段で維持する保証はない。
CPUメモリーを介さない制約は維持する。検証対象はネイティブ版のMPEG-2であり、
他のGPU・コーデック・配布パッケージへの保証ではない。

## 参照

- [qml6glsinkの形式とGL context共有](https://gstreamer.freedesktop.org/documentation/qml6/qml6glsink.html)
- [NVDEC MPEG-2のGLMemory NV12出力](https://gstreamer.freedesktop.org/documentation/nvcodec/nvmpeg2videodec.html)
- [greedyhの停止処理・前フレーム参照（1.28.2）](https://github.com/GStreamer/gstreamer/blob/1.28.2/subprojects/gst-plugins-base/ext/gl/gstgldeinterlace.c)
- [OpenGLデインターレース](https://gstreamer.freedesktop.org/documentation/opengl/gldeinterlace.html)
- [VAデインターレース](https://gstreamer.freedesktop.org/documentation/va/vadeinterlace.html)
- [VAのcontext共有と状態遷移](https://github.com/GStreamer/gstreamer/blob/1.28.2/subprojects/gst-plugins-bad/sys/va/gstvabasetransform.c)
- [DMABufのmodifier交渉とGL import](https://gstreamer.freedesktop.org/documentation/additional/design/dmabuf.html)
