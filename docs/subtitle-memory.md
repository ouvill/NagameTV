# 固定文字列による字幕メモリー計測

実放送のheaptrackで見つかった字幕描画の確保を、同じ入力で調べるための手動計測。
本番のSubtitleOverlay、SubtitleGlyph、subtitleOutlinePathを使用する。
表示部品の代用品やGStreamerは使用しない。描画方式の変更も行わない。

```bash
DISPLAY=:0 python3 scripts/benchmark-subtitle-memory.py benchmark/subtitle-memory-new-run
```

出力先は新規ディレクトリーを指定する。スクリプトはX11・GPUデバイス・GLXを検出、
検証してから実行し、ソフトウェア描画へのフォールバックは行わない。
Qtの開発用pkg-config設定とC++コンパイラーを使い、既存の描画試験ヘルパーをビルドする。
音声、放送、キー・ポインター入力は使わない。仮想ディスプレイのUI試験とは別の計測である。

1440×810のウィンドウ、960×540の字幕座標、通常書体・縁取りON・glyphHeight 36で、
32セルを1画面に表示する。U+4E00〜U+4EFFの256文字を3周し、続いて
U+4F00〜U+4FFFの別の256文字を2周する。各周回後に字幕を消去する。
変化後の描画をwaitForRenderingで確認し、500ms待ってから採取マーカーを出す。
文字列履歴を計測側で保持せず、次の32セル分だけを生成する。

別プロセスの採取側が各マーカーで/procのstatusとsmaps_rollupを保存する。
RSS・PSS・匿名ページ等をsamples.jsonへ記録し、Qtログ・renderer情報も保存する。
全11段階の順序と正常終了を検証し、60秒超過・1MiBを超えるログ・不完全な採取は失敗にする。
失敗時も自分が起動したヘルパーを回収し、途中のログと採取結果を保存する。

mallocのmmapしきい値は、アプリの128KiB固定に合わせて子プロセスのglibc起動設定で指定する。
他のglibc tunablesは維持する。アプリのmalloptによる設定そのものの試験ではない。
アロケーター統計や割り当てスタックはこの計測では採取せず、RSSの差を生存オブジェクト量や
リーク量に置き換えない。RSSの上限値をテストの合否基準にはしない。

## NVIDIAでの採取結果（2026-09-07）

Qt 6.10.2、RTX 4070 Ti / 595.84、X11 :0、OpenGLで最終スクリプトを実行した。
PID 149445、全11段階の採取成功、試験3件成功、正常終了0。
各周回終了時（消去前）のRSSは次の通り。

| 段階 | RSS MiB |
| --- | ---: |
| フォントロード後・字幕表示前 | 100.21 |
| 最初の256文字・1周目 | 163.23 |
| 同じ256文字・2周目 | 163.80 |
| 同じ256文字・3周目 | 163.90 |
| 別の256文字・初回 | 201.73 |
| 別の256文字・繰り返し | 201.41 |

最終消去後は199.84MiB。新しい文字集合では増加し、同じ文字集合の繰り返しでは
増加が小さいという結果になった。文字に対応する描画資源やキャッシュが増える可能性と
整合するが、その所有者・解放条件や長時間リークの有無をこのRSS計測だけでは確定できない。
256〜512文字・約10秒の試験であり、全Unicode、複数書体・サイズ、ウィンドウ変更、
全機能を併用した長時間再生の代わりにはならない。

初回の試作では、周回内ですでに確認した描画を、変更せずもう一度waitForRenderingで
待って失敗した。この試行は成功した測定に含めず、待機を変更直後へ限定した。
[Qt TestCaseの待機仕様](https://doc.qt.io/qt-6/qml-qttest-testcase.html#waitForRendering-method)
を参照。修正後の256文字3周、512文字5周を実行し、さらにglibcの固定しきい値を
指定した測定へ揃えた。試作の既定アロケーター値は上表に混ぜていない。

最終証跡はGit対象外benchmark/subtitle-repeat-final/のsamples.json、player.log、
renderer.txt、各段階のstatus/smaps_rollupとヘルパーバイナリー。
過程はbenchmark/subtitle-repeat/のattempt1、first256、default-allocator等に保存した。
既存の長時間再生PID 78793/132581は維持した。ユーザーの画面へ入力は送っていない。

## 縁取りだけを除く比較（2026-09-07）

```bash
DISPLAY=:0 python3 scripts/benchmark-subtitle-memory.py --no-outline benchmark/subtitle-memory-no-outline-new-run
```

追加した--no-outlineは試験入力のstrokedだけをfalseにする。文字本体のNativeRendering、
書体・サイズ・座標・色と表示順は保持する。本番の字幕設定やQMLは変更しない。
条件はconditions.jsonに保存する。環境変数は試験ヘルパーだけが読み、QMLへboolを渡す。
通常の描画試験はこの追加プロパティを使わず、従来の入力を維持する。

最終比較ではONのPID 150167とOFFのPID 149996がそれぞれ正常終了し、
11段階の採取と試験3件が成功した。縁取り生成の呼び出しはONで1280回、OFFで0回。
OFFで処理が走っていないことも試験で検証した。

| 段階 | 縁取りON RSS MiB | 縁取りOFF RSS MiB |
| --- | ---: | ---: |
| 表示前 | 100.12 | 100.30 |
| 最初の256文字・1周目 | 162.80 | 155.29 |
| 最初の256文字・3周目 | 163.42 | 155.35 |
| 別の256文字・初回 | 197.44 | 189.79 |
| 別の256文字・繰り返し | 197.22 | 189.82 |
| 最終消去後 | 195.98 | 189.82 |

縁取りを除いても、新しい文字での大きな増加と同じ文字の再利用時の小さな増加が残る。
したがってShapeの縁取りだけではこの現象を説明できない。文字本体の描画経路を
次の調査対象とし、字形・基準線を変えずに改善できるか検討する。
この短い比較から各モジュールの厳密な固定コストや長時間のリークを算出しない。
証跡はbenchmark/subtitle-outline-final/とbenchmark/subtitle-no-outline-final/。

## 文字本体の割り当てと描画方式の比較（2026-09-07）

固定512文字・縁取りOFFのヘルパーを、検証済みの同じ実GPUでheaptrack付き起動した。
全5周・縁取り生成0回・試験3件成功、正常終了コード0。SIGTERMによる打ち切りではない。
-m 0の個別スタックで、次のpeakを確認した（heaptrackの表示単位M）。

| 個別peak | 呼び出し経路 |
| --- | --- |
| 33.55M | QTextureGlyphCache::fillInPendingGlyphsからQByteArray::fillへの確保 |
| 25.17M | 同じ経路からQImageTextureGlyphCache::resizeTextureData、QImage::copyへの確保 |
| 13.16M・512回 | QTextureGlyphCache::populateからQFontEngineFT::loadGlyphへの確保 |

各peakの時刻は同じとは限らず、合計を全体使用量として扱わない。
全体peak heapは126.80M、heaptrack込みRSS peakは256.22M、終了時の未解放表示は8.54M。
--filter-bt-function QTextureGlyphCacheで終了時の未解放スタックを抽出すると該当なし。
この記録では、追跡された文字キャッシュ経由の確保は正常終了までに解放された。
Qt・ドライバーの他の未解放領域の性質や、長時間アプリ全体のリークを否定するものではない。

[Qt TextのrenderType仕様](https://doc.qt.io/qt-6/qml-qtquick-text.html#renderType-prop)を
確認し、SubtitleGlyphのrenderTypeだけを試験的に変更して同じ縁取りON入力を比較した。
QtRenderingは距離場、CurveRenderingは曲線による別の描画方式である。
文書のgraphics memoryに関する比較を、そのままプロセスRSSの大小とは解釈しない。

| 方式 | 5周目（2番目の256文字の再表示）のRSS MiB |
| --- | ---: |
| NativeRendering（元の方式） | 197.22 |
| CurveRendering | 238.79 |
| QtRendering | 203.22 |

各実行は全11段階の採取・試験3件・終了0を確認した。CurveRenderingでは既存の
実GPU字幕描画試験9件も成功したが、この固定入力ではメモリー削減にならなかった。
そのためrenderTypeの変更は採用せずNativeRenderingへ戻した。QtRenderingの
見た目比較は追加実施していない。いずれも約10秒・512文字の結果で、文字集合を増やした
長期の優劣を確定する比較ではない。稼働中アプリの描画方式は変更していない。

証跡はbenchmark/subtitle-native-heaptrack/のallocations.zst、allocators.txt、
glyph-cache-at-exit.txt、player.log、fixture.qmlとhelper.cpp。
方式比較はbenchmark/subtitle-curve-outline/、benchmark/subtitle-qt-outline/。
各ディレクトリーに試験時のSubtitleGlyph.qmlも保存した。
計測スクリプトも、未コミットの実験をHEADだけで取り違えないよう、本番字幕部品・
縁取りヘッダー・試験ヘルパー・固定入力・スクリプトの実ファイルをsources/へ保存する。
