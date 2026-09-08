# 普段の視聴からメモリ増加を調べる

通常起動はこれまでどおり。変更後は一度ビルドする。

```sh
workshop run dev build
workshop run dev run
```

通常の診断は10秒ごとと操作時に記録する。今回追加した `usage/history/` は
同じ計測値を最初と以降60秒以上経過した時点に別保存する。追加の /proc 読み取りはない。
GCログが大量に出て詳細ログが回転しても、疎な履歴には影響しない。
詳細・履歴それぞれ現行4MiBと前回4MiB、起動時に終了済みの6区画を保持する。
保存日数の保証はない。稼働中の別プロセスのログは整理しない。
履歴には毎回の操作イベントは残らないため、詳細ログも早めに保存する。

## 記録の切り替え

`run` と `profile-memory` は次のフラグを受け取る。
優先順位はコマンドライン指定、対応する環境変数、コマンドの既定値の順。
明示的な0を上書きしない。0/1以外や値なしは起動前にエラーとする。
同じフラグを複数指定した場合は最後の値を使う。

| フラグ | 環境変数 | runの既定 | profile-memoryの既定 |
|---|---|---|---|
| `--diagnostics=0/1` | `MIRAKURUN_DIAGNOSTICS` | 1 | 1 |
| `--gc-log=0/1` | `MIRAKURUN_GC_LOG` | 0 | 1 |
| `--heaptrack=0/1` | `MIRAKURUN_HEAPTRACK` | 0 | 1 |

```sh
# 解析用記録をすべてOFF
workshop run dev run --diagnostics=0 --gc-log=0 --heaptrack=0

# heaptrackだけ記録（通常診断・GCはOFF）
workshop run dev profile-memory --diagnostics=0 --gc-log=0

# heaptrackを使わず、GCだけJSONLに記録
workshop run dev run --diagnostics=0 --gc-log=1 --heaptrack=0

# アプリやデバイスを起動せず、解決後の設定を確認
workshop run dev profile-memory --diagnostics=0 --print-log-settings
```

diagnosticsはプロセス計測・操作スナップショットと1分履歴を制御する。
GCは独立しており、GCだけONならusageのJSONLにGC通知だけを保存し、historyは作らない。
両方OFFなら診断Recorderも作らない。既存の保存ファイルはOFF指定で削除しない。
GCがOFFなら、Qtの外部ログ設定で対象カテゴリが有効でもアプリの記録経路へ流さない。
ON時はQtのログ設定によってカテゴリが抑制される場合がある。
通常のエラー・警告ログはこの3フラグとは別で、標準エラーへの出力は `RUST_LOG` で制御する。

`run --features=...` は従来どおり、diagnosticsの明示指定がなければ既定OFF。
heaptrackのON/OFFだけを変えても他のフラグは変わらない。
`profile-memory --heaptrack=0` はプロファイル保存フォルダーやバイナリコピーを作らず、
通常の状態ディレクトリーへ起動する。heaptrackはプロセス起動時にのみ切り替え可能。
これらのCLIフラグはWorkshopの起動スクリプト用。実行バイナリを直接起動する場合は
対応する環境変数を使い、heaptrackは外部コマンドとして起動する。

## 関数別の確保元も記録する

```sh
workshop run dev profile-memory
```

ビルド済みのアプリをheaptrack付きで起動する。初回から記録するので、増加後に
同じ操作を再現する必要がない。通常どおり視聴し、最後はアプリの終了操作で閉じる。
既存の視聴プロセスへの後付けアタッチは行わない。

現在のCargo設定は `--release` + `debug = 1`。最適化を維持して関数・行の情報を残す。
最適化なしのDebugビルドもheaptrackで計測可能だが、処理速度や待機量が変わりうる。
なおCMake側をDebugにするだけではRustはDebugにならない。現行CMakeは常に
`cargo build --release` を呼ぶ。

起動スクリプトはX11、GPUデバイス、ハードウェアOpenGL、PulseAudio接続を検証し、
不足・検証失敗時は終了する。heaptrack / xdpyinfo / glxinfo / pactl / timeout が必要。
この変更ではコンテナ構成やパッケージのインストールを変更していない。

保存先は `benchmark/memory/session-日時-一意名/`。以下を保存する。

- heaptrackの圧縮記録（拡張子はインストールされたheaptrackによる）
- 実際に起動した実行ファイルのコピー
- 有効にした診断・Qt GC通知のJSONL（`state/mirakurun-viewer/usage/`）
- 共有ライブラリのパス、GPU情報、heaptrackバージョン、Gitリビジョンと追跡ファイル差分
- 起動引数（NUL区切り）

Qt GCはこの起動で既定ON。`--gc-log=0` または `MIRAKURUN_GC_LOG=0` で無効にできる。
解決後の3つの設定はセッションの `log-settings.txt` に保存する。
heaptrack記録には通常ログのような容量上限・ローテーションを設けていない。
確保頻度によって記録負荷・ディスク使用量が大きくなるため、視聴の区切りで終了する。
自動削除はしない。保存した共有ライブラリ一覧はライブラリ本体やデバッグシンボルの
アーカイブではないため、Qt等を更新する前に解析するか対応するシンボルを別途保存する。
強制終了やディスク不足では末尾の記録が欠ける場合がある。

## 時系列レポート

通常起動の最新PIDを解析する（出力先は新規ディレクトリー）。

```sh
workshop run dev analyze-memory --output benchmark/memory-report-001
```

プロファイル起動の記録は入力を指定する。

```sh
workshop run dev analyze-memory \
  --input benchmark/memory/session-日時-一意名/state/mirakurun-viewer/usage \
  --output benchmark/memory-report-002
```

`--pid 12345` で対象を指定できる。HTMLは `index.html`。Python標準ライブラリのみ使用し、
生成時にディスプレイ・GPU・音声は不要。ブラウザーで開くと、RSS/PSS/匿名メモリ、glibc、
字幕・コメント・EPG保持量、スレッド・FD、RSS増加上位区間と操作イベントを確認できる。
元ログも `sources/` にコピーし、起動時整理から独立して保存する。
稼働中のファイルコピーは原子的ではない。末尾の不完全行は数を報告して読み飛ばす。
可能なら終了後に解析する。詳細と履歴の重複サンプルは統合する。

関数別の内訳は保存したheaptrackファイルを `heaptrack_print FILE` または
`heaptrack_gui FILE` で解析する。GUIの起動には表示環境が必要。
Qt内部の関数・行まで調べるには、使用中のQtに一致するデバッグシンボルも必要。

RSSの増分をすべてリークとは扱わない。glibc空き領域の保持やキャッシュもあり、
heaptrackはGPU専用メモリやすべての直接mmapを網羅しない。通常ログのRSSと
allocator統計、heaptrackの生存確保を合わせて判断する。
