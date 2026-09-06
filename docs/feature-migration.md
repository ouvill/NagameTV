# mainの機能移植と検証状況

2026-09-07。参照元は `/project` のmain。README、QML、Rustの各モジュールと
viewer-coreを照合し、機能単位で移植する。表の「一部」はmainの代替が未完成であることを示す。
元のmain worktreeのユーザー変更には手を加えない。

| 機能・契約 | mainの参照元 | 後継ブランチの状況・残作業 |
| --- | --- | --- |
| Mirakurun接続・ライブ再生・選局 | playback、runtime、channels | 基本動作あり。異常時の詳細UIは未移植 |
| チャンネル分類・リモコン順・ロゴ | viewer-core/channels、QML | 分類・番号順・絞り込み、ロゴ付きブラウザーとC開閉、カードの現在番組・時間・進行率を追加。番組カードの実画像検証、mainのデザイン再現、実況勢い、EPG連動の同時放送サブ局整理は未完了 |
| 字幕PTS同期・表示期限・選局時破棄 | subtitles | 移植済みの同期を回帰検証。実放送での再検証も継続 |
| 字幕の書体・縁取り・配置再現性 | subtitle_outline.h、QML | mainの同梱ARIBフォント・基準線を保持する輪郭描画を移植。Qt描画試験済み、実放送と長時間併用の検証を継続 |
| EPG全局取得・定期更新・更新失敗時保持 | epg、runtime、viewer-core | 基本取得あり。選択局の指定日だけを投影。旧200件制限を廃止 |
| EPG時間軸・複数局表示・詳細・現在番組 | QML、epg、epg_events | 現在番組・進行率・詳細、7日分の日付選択、予定番組の詳細を追加。複数局の時間軸は未移植 |
| 複数音声・主/副/主副・言語の照合 | audio、transport、viewer-core/audio | 未移植。推測せず放送メタデータと照合する |
| 音量・ミュート | playback、QML | 音量復元とミュートUIを移植。型付き出力状態とnativeプロパティ保持を試験。実再生音声の確認は未実施 |
| 実況接続・チャンネル追随・描画・調整 | comments、runtime、QML | 未移植。無効時は通信と描画を生成しない |
| 接続先・局・音量・字幕の設定保存 | settings、viewer-core/settings | 互換TOMLの読み書きを追加。現状は正常終了時に保存 |
| 言語設定・動的翻訳切替 | localization.h、translations、QML | 未移植。既存のlanguage設定は保持 |
| 自動再生・環境変数による上書き | runtime、README | SERVER/SERVICE_ID/AUTOPLAY対応を追加 |
| デインターレース設定 | playback、README | YADIF/Linear/Offの起動設定を移植。型検証とCPU試験済み |
| 全画面・自動非表示・ウィンドウ操作・ショートカット | QML、pointer_activity.h | 全画面、C、G、PgUp/PgDown、Escape、操作部の重ね合わせと3.2秒後の自動非表示を追加。入力欄・ポップアップとの競合、監視の解放を検証。独自ウィンドウ枠は未移植 |
| エラー種別表示・復旧操作・詳細コピー・診断保存 | viewer-core、runtime、QML、diagnostics | 型付きエラーあり。main相当の案内・操作・保存は未移植 |
| 動画統計・表示中のみ収集 | video_stats、QML | 表示中のみ1秒ごとに収集する統計パネルを移植。長時間併用試験は未実施 |
| 診断ログ・継続的な資源測定 | diagnostics、scripts | 簡易allocator計測あり。配布向け診断は未移植 |

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
言語・実況など未移植の設定項目はTOML値として保持してから再保存する。

設定は変更がある場合のみ正常終了時に保存する。クラッシュ時の保存や同時起動間の
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

起動時の組み立ては `player/startup.rs` に分離した。既存Player制御全体の分離、
字幕等の既存unwrapの監査、機能移植後の長時間メモリー／性能検証は引き続き必要。

デインターレースと動画統計の設計・検証は [video-statistics.md](video-statistics.md)。

チャンネルの分類・番号順・種別絞り込みの契約と検証は [channel-selection.md](channel-selection.md)。

字幕の書体・輪郭描画の設計と検証は [subtitle-rendering.md](subtitle-rendering.md)。

音量・ミュートの設計と検証は [audio-output.md](audio-output.md)。

現在番組の検索・詳細表示とEPGのデータ整理は [current-program.md](current-program.md)。

番組表の日付選択・予定番組の詳細は [guide-calendar.md](guide-calendar.md)。

全画面とショートカットの責務・検証は [window-actions.md](window-actions.md)。
