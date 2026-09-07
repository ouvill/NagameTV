# mainの機能移植と検証状況

2026-09-07。参照元は `/project` のmain。README、QML、Rustの各モジュールと
viewer-coreを照合し、機能単位で移植する。表の「一部」はmainの代替が未完成であることを示す。
元のmain worktreeのユーザー変更には手を加えない。

| 機能・契約 | mainの参照元 | 後継ブランチの状況・残作業 |
| --- | --- | --- |
| Mirakurun接続・ライブ再生・選局 | playback、runtime、channels | 基本動作あり。局一覧取得を型付きの終了待ちへ分離し、旧要求終了後に最新接続先を取得。局一覧の5分更新・一覧表示時の60秒条件・失敗画面からの強制更新を追加。正常な空一覧と取得失敗を区別し、空の期間を挟む選択復元を修正。停止・再生失敗画面、再試行、選択可能なエラー詳細を追加。実サーバー停止／復旧の検証を継続 |
| チャンネル分類・リモコン順・ロゴ | viewer-core/channels、QML | 分類・番号順・絞り込み、ロゴ付きブラウザーとC開閉、カードの現在番組・時間・進行率を追加。下部・右側のカードをmainの寸法・配色へ移植し実画像確認。同時放送サブ局除外を両一覧・前後選局へ追加。実況勢いの表示とmain相当のホイール移動を追加。画面全体の比較と長時間検証は未完了 |
| 字幕PTS同期・表示期限・選局時破棄 | subtitles | 移植済みの同期を回帰検証。字幕にも明示的な放送serviceIdを渡し、配信用IDからの推測を廃止。実放送での再検証も継続 |
| 字幕の書体・縁取り・配置再現性 | subtitle_outline.h、QML | mainの同梱ARIBフォント・基準線を保持する輪郭描画を移植。Qt描画試験済み、実放送と長時間併用の検証を継続 |
| EPG全局取得・定期更新・更新失敗時保持 | epg、runtime、viewer-core | 基本取得あり。番組表へは指定日分の全局を投影。旧200件制限を廃止。借用ビューからJSONを直接生成し、全セル分の中間配列を除去。選択局だけの変更による全局JSONの再生成も除去 |
| EPG変更イベントの購読 | epg_events、runtime | 上限付き逐次解析・60秒の集約・HTTP再接続を分離し、EPG有効時の購読と再取得へ接続。旧購読の終了後に最新接続先を起動する。取得中のEPG更新要求も一回に集約。ローカルHTTP検証済み、実Mirakurunでの反映・長時間検証は残る |
| EPG時間軸・複数局表示・詳細・現在番組 | QML、epg、epg_events | 現在番組・進行率・詳細、7日分の日付選択、予定番組の詳細を追加。全画面の複数局時間軸と画面外列の解放を追加。mainのジャンル配色を追加。上部ツールバーの配置・操作接続を追加。詳細からの視聴要求を現在EPGで再検証する経路を追加。実選局の確認とmainとの全体デザイン一致は未完了 |
| 複数音声・主/副/主副・言語の照合 | audio、transport、viewer-core/audio | ネイティブ音声トラックのID選択とmain相当の音声メニューを追加。PMTのcomponent_tag取得を字幕・EPGから独立して追加。現在番組の音声情報を一意なタグで照合し、言語・主／副の役割表示を追加。二重音声の片側出力・3モード選択を追加し、生成PCMによる出力と形式更新・flush時の解除を検証。主音声の初期選択と手動選択の優先を追加。二重音声・複数トラックの実放送検証も残る |
| 音量・ミュート | playback、QML | 音量復元とミュートUIを移植。型付き出力状態とnativeプロパティ保持を試験。実再生音声の確認は未実施 |
| 実況接続・チャンネル追随・描画・調整 | comments、runtime、QML | 受信・接続切り替え・再接続・200件上限の履歴一覧・有効無効設定を組み込み。流れる実況と表示調整も組み込み。mainの独立した再生設定ポップアップと終了時保存を追加し、調整部品を共有化。勢い表示用の上限付き解析・5分間隔取得・無効化時のキャンセル・2か所の一覧表示を追加。mainの投稿欄風表示・鉛筆／送信アイコンも補完（main同様に投稿処理なし）。mainとの実画面比較と実サービス／長時間検証は未完了 |
| 接続先・局・音量・字幕の設定保存 | settings、viewer-core/settings | 互換TOMLの読み書きを追加。接続・選局・機能変更、音量操作確定、設定画面を閉じた時と正常終了時に変更分を保存 |
| 言語設定・動的翻訳切替 | localization.h、translations、QML | mainの108項目のカタログ・Qt切り替えヘルパー・リソース生成を移植。QtCore/QML試験で再翻訳と翻訳器寿命を確認。型付き言語設定・保存・言語選択UIと既存カタログに対応する文言へ接続。QML固定文言47項目も補完。日付ロケール連動と実況・EPG状態の翻訳を追加。字幕の状態・失敗理由も型付きで翻訳対応。通常の再生・接続状態も型付きで翻訳対応。他の低層エラー・診断文の翻訳と英語レイアウト実検証は残る |
| 自動再生・環境変数による上書き | runtime、README | SERVER/SERVICE_ID/AUTOPLAY対応を追加 |
| デインターレース設定 | playback、README | YADIF/Linear/Offの起動設定を移植。型検証とCPU試験済み |
| 全画面・自動非表示・ウィンドウ操作・ショートカット | QML、pointer_activity.h | 全画面、C、G、PgUp/PgDown、Escape、操作部の重ね合わせと3.2秒後の自動非表示を追加。入力欄・ポップアップとの競合、監視の解放を検証。独自枠とシステム移動・リサイズ要求を追加。外部キー入力と実環境の移動・リサイズは未検証 |
| エラー種別表示・復旧操作・詳細コピー・診断保存 | viewer-core、runtime、QML、diagnostics | 型付きエラー、停止／失敗案内、再試行と選択可能な詳細表示を追加。mainのHTTPコード別案内を移植し、失敗表示中の日英切り替えに対応。最新再生エラーの64KiB上限保存とログフォルダーを開く設定ボタンを追加。実再生エラーからの保存、デスクトップ連携の実動作は未検証 |
| 動画統計・表示中のみ収集 | video_stats、QML | 表示中のみ1秒ごとに収集する統計パネルを移植。mainの表示領域／DPR・閉じる操作・配置と配色を補完。番組表表示中は統計パネルを解放。実画面と長時間併用試験は未実施 |
| 診断ログ・継続的な資源測定 | diagnostics、scripts | 簡易allocator計測あり。配布向けの計測・サイズ上限付きログ保存を単独crateへ分離。32件上限の記録ワーカーと終了待ちを追加。終了済みログの6区画保持を追加。GC通知の上限付き受け口を追加。Qtハンドラー登録・定期／操作イベント記録とアプリへの接続を追加。実GC通知・実画面とのカウンター照合と長時間負荷測定は未検証 |

動作仕様の差を見つけたらこの表を追加・修正する。表だけを根拠に機能互換と判断せず、
各機能の型・エラー・終了条件、CPUテスト、実機の検証記録を揃えてから移植済みとする。
mainの全面置き換えは、未移植／一部の項目と全機能併用時の長時間検証が残っているため未完了。

## 設定保存の設計

`settings/model.rs` はPreferencesと有限・範囲内のVolume型、選択局の照合、環境上書きの
規則を持つ。QtやファイルIOに依存しない。`settings/mod.rs` が64KiB上限の読み込みと
同一ディレクトリーの一時ファイルからの置換を担当する。
音量はmainと互換な0〜100のTOML値で保存し、Qt/GStreamerへ渡す境界で0〜1へ変換する。

通常起動はXDG_CONFIG_HOMEまたはHOME/.configのmirakurun-viewer/settings.tomlを読む。
接続先が変更された場合は前サーバーの保存局IDを消し、SERVICE_IDの明示指定を優先する。
AUTOPLAY=1の場合だけ一覧取得後に再生する。未指定時は選択局を復元して待機する。
EPGは通常起動では既定ON、字幕は設定値を復元する。

`--features=...` は永続化するパスを持たないTransientセッションとなり、既存設定を
読み書きしない。比較実験の音量50%、明示機能のON/OFFと環境変数を使う。
読み込み失敗もエラーを表示してTransientにし、壊れたファイルを初期値で上書きしない。
言語と実況の表示設定は型付きフィールドへ移行済み。その他の未知の設定項目はTOML値として保持してから再保存する。

設定は変更がある場合のみ操作確定時・設定画面を閉じた時・正常終了時に保存する。クラッシュ時の保存や同時起動間の
設定マージは現段階では保証しない。状態保存・IOを毎フレームのpollに追加しない。

参照したAPI:
- [Qt Slider moved](https://doc.qt.io/qt-6/qml-qtquick-controls-slider.html#moved-signal)：ユーザー操作時のみRustへ音量変更を通知し、復元はvalueのバインディングで行う。
- [tempfile NamedTempFile::persist](https://docs.rs/tempfile/latest/tempfile/struct.NamedTempFile.html#method.persist)：同一ディレクトリーの一時ファイルで置換し、失敗時はRAIIで一時ファイルを破棄する。renameとファイルsyncを使うが、電源断後のディレクトリー永続化まで保証するものではない。

## 設定移植の検証結果

自動テスト33件成功（外部TSが必要な任意テスト1件は未実行）、Clippy全ターゲットで警告なし、
CMakeビルド成功。設定テストはmain形式の読み込み・未知項目の再保存・有限音量への正規化・
環境上書きと局照合・64KiB上限・保存失敗時の元データと一時ファイルの後始末を検証した。

実機では独立したXDG_CONFIG_HOMEに互換TOMLを用意し、次を新規プロセスで確認した。

1. 保存局`3209641985`で自動再生し、前の局`3209641984`へ移動。音量を25%から約53.1%へ
   変更し、EPGをOFFにして正常終了。局ID・音量・EPGがTOMLへ保存された。
2. 再起動して`3209641984`で再生。音量スライダーの位置とEPG OFFの復元を画像で確認。
3. 同じ設定ディレクトリーで`--features=none`を起動。EPG通信なし、設定ファイルのバイト列は不変。

全プロセスが正常終了。言語jaとcomment_speed=1.25の未知項目を保持した。
証跡は `benchmark/settings-migration/` のrun.py、result.log、各ログと画像。
ユーザーの通常設定ディレクトリーはこの検証で書き換えていない。

起動時の組み立ては `player/startup.rs` に分離した。EPG接続、字幕表示、局一覧取得の終了待ちも個別モジュールへ分離済み。接続・選局・局一覧反映はplayer/connection.rsへ分離した。Player本体の再生調整、既存unwrapの監査、機能移植後の長時間メモリー／性能検証は引き続き必要。

## 操作時の設定保存

mainの音量Sliderの押下終了・SettingsDrawer終了・サーバー変更・選局時の保存を移植した。
Playerの保存処理はplayer/preferences.rsへ集約し、正常終了も同じ経路を使う。
字幕・EPGの切り替えも確定した値を保存する。

[Qt Sliderの仕様](https://doc.qt.io/qt-6/qml-qtquick-controls-slider.html#pressed-prop)
ではpressedはマウス・タッチ・キー操作を含む。ドラッグ中は音量適用だけを行い、
押下終了時に保存する。pressed変化を伴わないwheel等のmovedは400msの単発Timerでまとめる。
新しい押下でTimerを止め、終了中のUIからは保存を要求せずshutdownで最終値を保存する。
値のバインディング更新だけではmovedは発生しない。

Session::flushの結果をSaved / Unchanged / Transientで表す。最後の保存値と一致する場合、
シリアライズやファイル操作は行わない。保存に失敗した場合は最後の保存値を更新せず、
次の確定操作で再試行できる。成功時は設定エラーを消すが、読み込み失敗や実験用起動の
Transientは書き込みを行わず、元の読み込みエラーも消さない。
未知のmain設定項目・64KiB上限・同じディレクトリーの一時ファイルとrenameは維持する。

設定を変更するたびの書き込みや継続Timer、設定スナップショットの履歴は追加していない。
保存自体はmain同様に同期IOで、sync_allもGUIスレッドで行う。遅いストレージでの
実GUIでの停止時間は未測定。下記の隔離した保存時間測定を踏まえ、必要なら単一ワーカーで最新値をまとめる方式を検討する。
ユーザーの通常設定ファイルへは、この検証から書き込んでいない。

CPU試験ではSessionが生存中に別Sessionで保存内容を読めること、連続編集の最終値だけの
保存、変更なしでIOを省くこと、保存失敗後の再試行、Transientの区別を検証する。
マウス・キー・wheelの実操作、SettingsDrawer終了時の保存、追加したQtメソッドの
実アプリ起動は音声出力の復旧待ちで未検証。

設定CPU試験5件成功、全ターゲットClippyは警告なし。fmt・diff検査とreleaseビルドも成功。
通常設定の読み書き、
エラー表示の実操作、400msの集約動作と保存時のフレーム時間はまだ検証していない。

デインターレースと動画統計の設計・検証は [video-statistics.md](video-statistics.md)。

チャンネルの分類・番号順・種別絞り込みの契約と検証は [channel-selection.md](channel-selection.md)。

字幕の書体・輪郭描画の設計と検証は [subtitle-rendering.md](subtitle-rendering.md)。

音量・ミュートの設計と検証は [audio-output.md](audio-output.md)。

現在番組の検索・詳細表示とEPGのデータ整理は [current-program.md](current-program.md)。

番組表の日付選択・予定番組の詳細は [guide-calendar.md](guide-calendar.md)。

全画面とショートカットの責務・検証は [window-actions.md](window-actions.md)。

音声トラック選択の設計・検証と未移植部分は [audio-selection.md](audio-selection.md)。

実況のプロトコル分離・接続と表示の残作業は [comments-migration.md](comments-migration.md)。

再生エラー保存とデスクトップ連携の仕様・検証範囲は [error-log-migration.md](error-log-migration.md)。

継続的な資源診断の分離と残作業は [resource-diagnostics-migration.md](resource-diagnostics-migration.md)。

言語切り替えの移植状況とQtの検証範囲は [localization-migration.md](localization-migration.md)。

EPG変更通知の解析・集約と購読の残作業は [epg-event-stream.md](epg-event-stream.md)。


## 字幕ネイティブ依存のビルドエラー

libaribcaption-sysのbuild.rsをResultを返す形へ変更した。Cargo環境変数の取得、ソースの
canonicalize、bindgenのResult、生成ファイルの書き込みからunwrap/expectを除去し、
失敗の対象パスと元の原因をエラーに含める。必須資源の不足時にビルドを継続しない。
cmake・cc・bindgen内部で発生するpanicまでResultへ変換したものではない。

コンパイル済みbuild-scriptを隔離した一時パスで実行し、CARGO_MANIFEST_DIR欠落、
OUT_DIR欠落、存在しないARIBCAPTION_SOURCE_DIRの3条件で終了コード1と原因付きエラーを
確認した。パニック出力なし。通常ビルドでは字幕デコードの2試験が成功した。
この変更はビルド時のエラー処理であり、再生時のメモリー改善を示すものではない。


## 設定保存の隔離測定

2026-09-07、worktreeと同じZFS上でreleaseビルドの保存処理を測定した。
SETTINGS_BENCH_DIR内に一時ディレクトリーを作り、通常設定は読み書きしない。
サービスIDを毎回変えて保存100回、各保存直後に変更なし確認100回を行い、最終値99も
再読み込みで確認した。一時ファイルは測定終了で削除する。

| TOMLサイズ | 保存中央値 | 保存p95 | 保存最大 | 変更なしp95 | 変更なし最大 |
| --- | --- | --- | --- | --- | --- |
| 246 bytes | 1,397µs | 1,498µs | 5,168µs | <1µs | <1µs |
| 61,709 bytes | 1,715µs | 1,838µs | 1,996µs | 1µs | 2µs |

数値は一回の実行であり、他のストレージ、負荷、実再生中のフレーム時間を代表しない。
保存時間にはTOML生成・一時ファイル・sync_all・置換・保存スナップショット更新を含む。
0µs表示はマイクロ秒未満の切り捨て。最大5.17msはゼロコストではないが、この測定だけを
根拠にバックグラウンド保存へ変更せず、実再生時の計測を次の判断材料とする。

再現コマンド（保存処理だけの明示的な任意試験）：

```sh
SETTINGS_BENCH_DIR=/home/workshop/qt-gstreamer-features CARGO_TARGET_DIR=build/cargo cargo test --release --locked --manifest-path rust/Cargo.toml measure_explicit_save_latency -- --ignored --nocapture
```


現在のCPU全体回帰は93成功・2任意試験除外。並列実行で見つかった音声試験のflush修正と、
未解明の字幕試験終了待ちの記録はaudio-selection.md末尾を参照。
全テスト成功の再試行だけを根拠に、初回の終了待ちや実機検証を完了扱いしない。

字幕のCPU試験には停止段階の任意stderr記録と実装側に合わせた明示的な解除を追加した。全体20回で再現なしだが、元の終了待ちの原因は未確定。調査コマンドと検証範囲はaudio-selection.md末尾に記載。


## 最近の変更をまとめた回帰確認

`2588c4c`時点の実装で、接続要求の受付／事前検証、番組表の更新条件、字幕選択、
実況の逐次JSON化を含めてRust全体を再実行した。96件中94成功・2ignored。
未実行は外部TS指定の字幕試験と任意の設定保存時間測定。以前終了待ちになった
字幕時計の試験も今回は終了したが、元の原因が判明したことは意味しない。

`rust/qml`直下のMain.qmlを除く39部品と`rust/qml/tests`の15ファイルは
qmllintがすべて終了コード0。Main.qmlは生成されるQt/GStreamer型への依存があるため
この単独lintの集計に含めない。直前の本体変更ではCMakeによるQML事前コンパイルを
確認済みだが、今回の静的検査は実GUIの生成・入力試験を代替しない。

残る完了条件は、mainとの全体デザイン比較、実サーバー停止／復旧と選局、
字幕と二か国語の実放送、EPGイベントと実況の実接続、ウィンドウ操作、
全機能併用の長時間メモリー／性能検証。個々の追加実装は表の範囲で評価し、
今回の成功件数だけでmainの置き換え完了とは判断しない。

## Workshop再起動後の実描画検証（2026-09-07）

再起動後、PulseAudio/PipeWireへの接続、GStreamerのpulsesinkへの短い無音出力、
X11接続、NVIDIA GeForce RTX 4070 Tiによる直接OpenGL描画を確認した。
音声デバイスへの出力成功は、放送音声を聴取できたことを意味しない。

実X11/OpenGL環境で以下を実行した（初期化・終了処理も成功件数に含む）。

- `scripts/test-subtitle-rendering.sh`: 9成功。字体・輪郭の画素、リサイズ、
  字幕消去と表示無効化時のdelegate解放、不要な輪郭再生成の抑制。
- `scripts/test-pointer-activity.sh`: 3成功。native入力監視の寿命とウィンドウ変更。
- `qmltestrunner -input rust/qml/tests`: 71成功。設定接続の受付／拒否、番組表、
  選局UI、ウィンドウ操作などの部品試験。Rustバックエンドの実接続試験ではない。

設定画面の言語選択アイコンだけ単体試験でqrcを解決できない警告が出たため、
既存の閉じるアイコンと同様にURLプロパティを公開し、試験では同じSVGのファイルURLを
指定した。修正後のSettingsDrawer試験は5成功、警告なし。
実アプリでは従来と同じqrc URLを既定値として使用する。

同じreleaseビルドを通常設定で起動し、`MIRAKURUN_AUTOPLAY=1`と
`MIRAKURUN_SERVICE_ID=3272402080`で関西テレビを再生した。
ログのPipeline PLAYING、時間を隔てた画像で映像の変化、PulseAudioの
mirakurun-viewer専用sink-input（float32le/2ch/48000Hz、corked=false、mute=false）を
確認した。通常の設定では字幕・実況は無効。番組情報の取得も成功している。
チャンネルボタンの実クリックでロゴ・番組情報付き一覧が開くことも確認した。

一方、この実行ではxdotoolによるC/Escape/F11送信が画面に反映されなかった。
X11上のfocus/active windowは対象アプリだが、マウスクリックは届くため、
入力送信経路と実アプリのキーハンドリングの切り分けが必要。
上記部品試験の成功から実アプリのショートカット成功を推定しない。
実音の聴取、選局反復、全機能併用・長時間RSS、およびmainとのデザイン比較は未完了。

キー操作の追跡では、別のX11イベント観測ウィンドウと最小Qt QWindowの両方で、
xdotoolのF11指定がControl付きで届くことを確認した。CとEscapeはQtへ修飾なしで届いた。
したがってF11送信失敗はアプリのショートカット実装だけでは判断できない。
さらにユーザーが同時に操作していたため、前回の実アプリへの入力試験は
フォーカス・画面状態が制御された条件ではなかった。C/Escapeの原因は未確定で、
操作時間を分けて再検証する。調査用ウィンドウを終了し、通常アプリへの外部入力を停止した。

## UIソース比較の追加確認（2026-09-07）

mainのMain.qmlと現在の分割QMLを照合した。以下はソース上の比較であり、
全画面の画像比較を完了したという記録ではない。

| 対象 | 確認内容 |
| --- | --- |
| 初期ウィンドウ | 1440×900、最小900×560、独自枠、Noto Sans CJK JPは同じ |
| 現在番組 | 左上24px、ロゴ64×36、見出し23px・2行、時間12pxを保持 |
| サイドパネル | 幅360〜408px、開いた時の映像領域縮小と16:9高さ制限を保持 |
| 動画統計 | 最大幅510px、余白14px、行間7px、項目ラベル142px、背景・枠色を保持 |
| ウィンドウタイトル | 固定のFeature Lab表記を現在番組名へ変更。番組情報がなければMirakurun Viewerへ戻す |

タイトルはCurrentProgramの既存解析済みprogramを参照するバインディングで更新する。
タイトル用のHTTP要求、JSON解析、タイマーは追加しない。Loader生成前と終了時の
item不在も既定タイトルへ戻す。参考は[Qt Window.title](https://doc.qt.io/qt-6/qml-qtquick-window.html#title-prop)と
[Loader.item](https://doc.qt.io/qt-6/qml-qtquick-loader.html#item-prop)。
QML事前コンパイルを含むreleaseビルドとdiff検査が成功。ユーザー操作中のアプリへの
キー・マウス入力や再起動は行っていないため、この変更の実タスク切替画面は未確認。

## 再生制御のモジュール分割

再生要求、READY停止、パイプライン状態のpoll、一度だけのHTTP再接続、最終終了を
`player/stream.rs`へ移した。`player.rs`はQt公開型・状態・プロパティ通知と
機能設定の接続を持ち、`player/connection.rs`は接続先・局一覧・選局を担当する。
ネイティブGStreamer操作は引き続き`playback`にあり、ここはQtと各機能の調整層。

移動前後のメソッド本体を比較し、一字一句同じであることを確認した。
変更した可視性は親・兄弟モジュールから使用するend_streamのpub(super)だけ。
READY成功前に字幕Sessionを破棄しない順序、停止失敗時のResult伝播、
復旧を一度に制限する条件、設定保存とワーカーのキャンセル順序を保持する。
この分割自体にメモリー使用量の改善は主張しない。

分割後のRust通常試験は97件中94成功・3任意試験除外。除外は外部TS字幕、
設定保存の手動計測、実サーバー取得JSONの検証。全ターゲットClippyも警告なし。
通常試験はEPGキャンセル・再取得、字幕時刻、音声トラック変更、停止時の
request pad命名保持を含むが、実GUIでの再生停止・選局反復を代替するものではない。
CMake releaseビルドも成功。起動中のアプリは再起動せず、外部入力も送っていない。

## 起動失敗時の通常解放

mainをExitCode返却、起動本体をResult<(), StartupError>へ変更した。
thiserrorのenumで引数・allocator・Qtアプリ・QML engine・再生初期化・翻訳・
QML生成の失敗を区別する。従来の引数エラー2、その他の起動エラー1を保持する。
Qtオブジェクト生成後のprocess::exitを除き、エラー返却時もengineとPlayerを
QGuiApplicationより先に通常のDropで解放する。Qtオブジェクトがnullの場合も
処理を黙って飛ばして成功終了せず、明示的な起動エラーにする。

[Rust process::exitの仕様](https://doc.rust-lang.org/std/process/fn.exit.html)では
スタック上のデストラクターを実行しないため、生成後の失敗に直接exitを使用しない。
Qt自身のabortやネイティブライブラリー内部の強制終了を捕捉する変更ではない。

## 自動操作用の画面分離

ユーザーは自動操作の再開を許可し、今後は普段の操作と重ならない仮想画面を希望した。
Xvfb :99を1440×900×24で起動し、glxinfo -Bで検証したところ、
Mesa llvmpipe・Accelerated:noだった。通常画面:0のNVIDIAとは異なるため、
GPU検証用としてアプリを起動せず、この確認用Xvfbは終了した。
GPU対応の仮想画面、またはCPU描画を使う試験範囲の明示が必要。
GPUのメモリー・性能をXvfbの結果で代替しない。
起動処理の変更はClippy全ターゲット・releaseビルド・diff検査が成功。
不正な機能引数でQt生成前に終了コード2となることを実行確認した。
Qt生成後の失敗注入と正常GUI起動は、この変更後には未検証。
