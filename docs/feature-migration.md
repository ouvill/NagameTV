# mainの機能移植と検証状況

2026-09-07。参照元は `/project` のmain。README、QML、Rustの各モジュールと
viewer-coreを照合し、機能単位で移植する。表の「一部」はmainの代替が未完成であることを示す。
元のmain worktreeのユーザー変更には手を加えない。

| 機能・契約 | mainの参照元 | 後継ブランチの状況・残作業 |
| --- | --- | --- |
| Mirakurun接続・ライブ再生・選局 | playback、runtime、channels | 基本動作あり。局一覧の5分更新・一覧表示時の60秒条件・失敗画面からの強制更新を追加。停止・再生失敗画面、再試行、選択可能なエラー詳細を追加。実サーバー停止／復旧の検証を継続 |
| チャンネル分類・リモコン順・ロゴ | viewer-core/channels、QML | 分類・番号順・絞り込み、ロゴ付きブラウザーとC開閉、カードの現在番組・時間・進行率を追加。下部・右側のカードをmainの寸法・配色へ移植し実画像確認。同時放送サブ局除外を両一覧・前後選局へ追加。実況勢い、画面全体の比較と長時間検証は未完了 |
| 字幕PTS同期・表示期限・選局時破棄 | subtitles | 移植済みの同期を回帰検証。字幕にも明示的な放送serviceIdを渡し、配信用IDからの推測を廃止。実放送での再検証も継続 |
| 字幕の書体・縁取り・配置再現性 | subtitle_outline.h、QML | mainの同梱ARIBフォント・基準線を保持する輪郭描画を移植。Qt描画試験済み、実放送と長時間併用の検証を継続 |
| EPG全局取得・定期更新・更新失敗時保持 | epg、runtime、viewer-core | 基本取得あり。選択局の指定日だけを投影。旧200件制限を廃止 |
| EPG時間軸・複数局表示・詳細・現在番組 | QML、epg、epg_events | 現在番組・進行率・詳細、7日分の日付選択、予定番組の詳細を追加。全画面の複数局時間軸と画面外列の解放を追加。mainのジャンル配色を追加。上部ツールバーの配置・操作接続を追加。詳細からの視聴要求を現在EPGで再検証する経路を追加。実選局の確認とmainとの全体デザイン一致は未完了 |
| 複数音声・主/副/主副・言語の照合 | audio、transport、viewer-core/audio | ネイティブ音声トラックのID選択とmain相当の音声メニューを追加。PMTのcomponent_tag取得を字幕・EPGから独立して追加。現在番組の音声情報を一意なタグで照合し、言語・主／副の役割表示を追加。二重音声の片側出力・3モード選択を追加し、生成PCMによる出力と形式更新・flush時の解除を検証。主音声の初期選択と手動選択の優先を追加。二重音声・複数トラックの実放送検証も残る |
| 音量・ミュート | playback、QML | 音量復元とミュートUIを移植。型付き出力状態とnativeプロパティ保持を試験。実再生音声の確認は未実施 |
| 実況接続・チャンネル追随・描画・調整 | comments、runtime、QML | 受信・接続切り替え・再接続・200件上限の履歴一覧・有効無効設定を組み込み。流れる実況と表示調整も組み込み。勢い表示用の上限付き解析・5分間隔取得・無効化時のキャンセル・2か所の一覧表示を追加。mainとの実画面比較と実サービス／長時間検証は未完了 |
| 接続先・局・音量・字幕の設定保存 | settings、viewer-core/settings | 互換TOMLの読み書きを追加。接続・選局・機能変更、音量操作確定、設定画面を閉じた時と正常終了時に変更分を保存 |
| 言語設定・動的翻訳切替 | localization.h、translations、QML | mainの108項目のカタログ・Qt切り替えヘルパー・リソース生成を移植。QtCore/QML試験で再翻訳と翻訳器寿命を確認。型付き言語設定・保存・言語選択UIと既存カタログに対応する文言へ接続。QML固定文言47項目も補完。日付ロケール連動と実況・EPG状態の翻訳を追加。字幕の状態・失敗理由も型付きで翻訳対応。通常の再生・接続状態も型付きで翻訳対応。他の低層エラー・診断文の翻訳と英語レイアウト実検証は残る |
| 自動再生・環境変数による上書き | runtime、README | SERVER/SERVICE_ID/AUTOPLAY対応を追加 |
| デインターレース設定 | playback、README | YADIF/Linear/Offの起動設定を移植。型検証とCPU試験済み |
| 全画面・自動非表示・ウィンドウ操作・ショートカット | QML、pointer_activity.h | 全画面、C、G、PgUp/PgDown、Escape、操作部の重ね合わせと3.2秒後の自動非表示を追加。入力欄・ポップアップとの競合、監視の解放を検証。独自枠とシステム移動・リサイズ要求を追加。外部キー入力と実環境の移動・リサイズは未検証 |
| エラー種別表示・復旧操作・詳細コピー・診断保存 | viewer-core、runtime、QML、diagnostics | 型付きエラー、停止／失敗案内、再試行と選択可能な詳細表示を追加。mainのHTTPコード別案内を移植し、失敗表示中の日英切り替えに対応。最新再生エラーの64KiB上限保存とログフォルダーを開く設定ボタンを追加。実再生エラーからの保存、デスクトップ連携の実動作は未検証 |
| 動画統計・表示中のみ収集 | video_stats、QML | 表示中のみ1秒ごとに収集する統計パネルを移植。長時間併用試験は未実施 |
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
言語など未移植の設定項目はTOML値として保持してから再保存する。実況の表示設定は型付きフィールドへ移行した。

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

起動時の組み立ては `player/startup.rs` に分離した。既存Player制御全体の分離、
字幕等の既存unwrapの監査、機能移植後の長時間メモリー／性能検証は引き続き必要。

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
UI停止時間は未測定であり、必要なら単一ワーカーで最新値をまとめる方式へ移す必要がある。
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
