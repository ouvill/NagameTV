# tsreadexによるPID変更素材の比較（2026-09-17）

同じ`recording-pid-change.ts`を事前にtsreadexで整形し、保存したファイルを製品の
Main.qmlと既存のファイル再生経路で再生した。アプリへの常時組み込みやパイプ接続は行っていない。
元TSも最後まで再生するが、整形後は位置の巻き戻り、今回の前後シークの失敗、音声の
未選択表示が解消した。実放送全般への保証ではない。

## 使用条件

- [tsreadex](https://github.com/xtne6f/tsreadex) commit `a82528ccb698fcd07b4da1bb2243e63d685c34a7`。
- `-n 1`で素材のサービスID 1を選択。ほかのオプションは既定値。
  音声補完・字幕のID3変換・SIの除去は指定していない。
- 入力: `tests/fixtures/recording-pid-change.ts`。約6秒、前半・後半各75フレーム、無音AAC。
- 出力: `benchmark/tsreadex/recording-pid-change-normalized.ts`。
- GStreamer 1.28.2。製品試験はX11・NVIDIA RTX 4070 Ti・PulseAudio接続を検出・検証して実行。

## 結果

| 観測 | 元TS | tsreadex整形後 |
| --- | --- | --- |
| 通常再生の終端 | 正常EOF | 正常EOF |
| 製品の位置の推移（100ms間隔で取得） | **2,955ms → 165ms**の減少を直接記録 | 減少を観測せず |
| 製品の終了位置 | 3,194ms | 6,394ms |
| 一時停止中の4,500 → 1,000 → 4,500msへのシーク | 165 → 1,000 → 1,936ms。後半の2回が不一致 | 4,500 → 1,000 → 4,500ms。全て一致 |
| シーク後の再開 | 映像出力と正常EOF | 映像出力と正常EOF |
| 製品の終了時の音声選択表示 | `selected: false` | `selected: true` |
| 製品のPulseAudio sinkへの通常再生のrender呼び出し | 284回 | 284回 |
| 独立CPU試験の映像出力バッファー | 旧PID 75 + 新PID 75 | 固定PID 149 |
| 独立CPU試験の音声出力バッファー | 旧PID 142 + 新PID 142 | 固定PID 284 |

前回は終了位置だけから位置の巻き戻りを推定していたが、今回は通常再生中の減少を記録した。
シーク失敗は`seek_to`の要求拒否ではなく、要求が受理された後に指定位置へ到達しないもの。
元TSの取得長さは終了時1,937ms、整形後は6,214msだった。整形後も終了位置との差が約180msあり、
長さ推定まで完全に一致するとは扱わない。元のバイトオフセットと整形後のオフセットも異なる。

音声は製品の`GST_DEBUG=basesink:6`ログから、通常再生開始からシーク試験開始までの
`pulsesink0`の`rendering object`を集計した。元TSでも後半を含む284回が出力されているので、
未選択表示を音声停止とみなす根拠はない。無音素材のため聴感や放送音声の品質は確認していない。
整形後の映像出力は元TSより1枚少ない。欠落なしを保証せず、この差の理由は未調査とする。

## 再現手順

リポジトリーのルートから実行する。以下の取得先は診断用の`benchmark/`以下で、
製品の依存やシステムへのインストールは追加しない。既に取得済みならcloneは省略する。

```sh
git clone https://github.com/xtne6f/tsreadex.git benchmark/tsreadex/source
git -C benchmark/tsreadex/source checkout --detach a82528ccb698fcd07b4da1bb2243e63d685c34a7
make -C benchmark/tsreadex/source
benchmark/tsreadex/source/tsreadex -n 1 tests/fixtures/recording-pid-change.ts > benchmark/tsreadex/recording-pid-change-normalized.ts

# CPUデコーダーとメモリーsinkのみ。表示・GPU・音声機器を使わない。
# Python PyGObject / Gst introspection / GStreamer libavが必要。
python3 scripts/fixtures/recording-output-audit.py --paced tests/fixtures/recording-pid-change.ts benchmark/tsreadex/recording-pid-change-normalized.ts

# 表示・GPU・音声機器を検証してから実行。元TSはシーク不一致でexit 1。
GST_DEBUG_NO_COLOR=1 GST_DEBUG=basesink:6 bash scripts/test-startup.sh recording-audit tests/fixtures/recording-pid-change.ts > benchmark/tsreadex/native-original.log 2>&1
# 整形後はexit 0。
GST_DEBUG_NO_COLOR=1 GST_DEBUG=basesink:6 bash scripts/test-startup.sh recording-audit benchmark/tsreadex/recording-pid-change-normalized.ts > benchmark/tsreadex/native-normalized.log 2>&1
```

入力SHA-256: `6109cf96c1cb693ee2dd456c196ab349b17dc513bbe88ee2348896dcbf6f8ed9`。
出力SHA-256: `72257872d2a82b39d9162bda446c145af359688a3a5cde4a3916d750a446c9ea`。
CPU試験はストリームIDごとにsink入力を計数し、リセットされる最終sink統計とは分けて出力する。
製品のauditは6秒の回復試験素材専用。通常再生の観測と、許容誤差500msの3回のシーク・
再開を行う。位置・音声選択の観測ログは別途評価し、exit 0だけを包括的な合格とは扱わない。

## 組み込みに向けて残る作業

今回の方式は処理済みファイルを保存するため、既存のランダムアクセスが利用できる。
tsreadexの標準出力をパイプで挟むだけでは同じシーク構成にはならない。
採用時は事前変換キャッシュの生成時間・容量・取消し・破棄、またはシーク可能な入力層を設計する。
実録画の音声切り替え・コーデック変更・字幕・番組情報保持と、パッケージへの同梱は未検証。

診断コードの検証は、上記CPU比較、製品audit（整形後成功・元TSは指定位置不一致）、
通常の`bash scripts/test-startup.sh`が成功。`cargo fmt --check`、`git diff --check`、
`cargo clippy --release --locked --all-targets --features native_tests -- -D warnings`も成功した。
