# 再生コアと任意機能の境界

このブランチは `minimal/qt-gstreamer` の `9fa758d` を出発点に、mainの機能とUIを
移植している。Rustの静的なモジュールで構成し、機能ごとの有効化・終了と資源所有を分離する。
全面置き換えは未完了。方針は [main-replacement.md](main-replacement.md)、
機能ごとの検証状況は [feature-migration.md](feature-migration.md) を参照。
以下の所有関係と状態変更の説明は2026-09-15の整理を反映したもの。
2026-09-17の[共通TS入力](ts-input-implementation.md)で、Sessionに元TSの受信・保持・
索引・Readerとシーク制御を追加した。PESヘッダー解析も`transport`へ共通化した。

## 所有関係

```text
main::run
 ├ QGuiApplication
 ├ diagnostics::Lifetime    記録ワーカーをQML engineの破棄後まで保持
 └ QQmlApplicationEngine
    └ PlayerRust（Qtへの投影と開始・停止順序）
       ├ playback::Session  再生と字幕世代の共通所有者
       │  ├ Playback        映像・音声、音声カタログ・PMT・主副出力
       │  └ Option<字幕Session> 購読・解析・同期時計
       ├ recording::Loader ローカルTS検証・取消し待ち・最新要求の所有
       ├ Acquisition        /api/servicesの取得・取消し待ち
       ├ ProgramInfo        /api/programsの取得・現行スナップショット
       ├ EPG Controller     番組変更通知の購読・停止待ち
       ├ Comments/Activity  実況接続・履歴と勢い取得
       ├ settings::Session  現在の設定・保存済みスナップショット
       ├ Network            Tokioランタイムと有限JSON用HTTPクライアント
       ├ Remote Control     任意のgRPC/gRPC-Web受付・有界操作キュー・状態通知
       └ diagnostics::Client 記録所有者への弱い参照
QML Loader                  字幕・番組表・流れる実況・統計表示の寿命
```

Playback本体は字幕デコーダーやEPGスナップショットを所有しない。Playbackと字幕の
寿命の連動はplayback/session.rs、再生のQt投影はplayer/stream.rs、EPGの通知消費と
投影はplayer/epg.rs、設定保存はplayer/preferences.rsに置く。
音声選択はplayback/audio_streams、PMT照合はaudio_components、主副の変換はaudio_routing。
PMTメッセージは通常のGStreamer bus pollで処理し、字幕の有効化には依存しない。
TS/PAT/PMTの構文とPSI再構成・PAT集約は`transport`で録画検証と字幕機能が共有する。
字幕用フレーミング・PES解析・字幕ES選択は字幕モジュールにある。
READYで停止してplaybinを再利用する方針を保持するが、パイプライン全体が最小版と同一ではない。

## 状態変更とQtへの通知（2026-09-15）

状態の事実はRustで所有し、QML向けの派生値には書き込み用フィールドを作らない。
`player/stream_state.rs`の`State`が停止・接続中・再生中・停止失敗を表し、
稼働中のvariantは再生対象を持つ`Attempt`を必須とする。
`Attempt::Live`は対象局と自動再試行の権利、`Attempt::File`は検証したローカルTSを保持する。
停止時の`State::Stopped`にも再再生対象を保持し、別フィールドの選択対象との同期を不要にする。
録画にはHTTP再試行や現在放送中の番組情報を適用しない。
`playing`・`connecting`・対象局・`recording`・録画名はこの状態から取得する。更新は
`change_stream_state`に集約し、状態全体を置き換えてからQtへ通知する。
一方の変更通知中に他方を読んでも更新途中の組み合わせにはならない。
停止失敗時は字幕の購読を解放せず、対象局も保持して次の明示操作で停止を再試行する。

途中再開拒否の再試行は、失敗したストリームの局と放送メタデータを引き継ぐ。
再試行済みのストリームへの重複した再生要求は、再試行の権利を復活させない。
`LiveAttempt`のフィールドと`Retry`は非公開で、AttemptにはCloneを実装しない。
再試行の取り出し自体が元の権利を消費するため、停止前にも二重取得できない。
`lifecycle::Status`は翻訳可能な説明文の状態として別に保持する。局一覧の通信失敗と
映像の再生継続は同時に成立するので、説明文から再生の状態を推測しない。

明示的な接続確認は`pending_server: Option<ServerUrl>`が形式検証済みの候補URLを所有し、
`loading`は候補の有無から取得する。候補URLの公開・取消し・完了通知は接続モジュールへ
まとめ、検証成功後の設定更新と保存は`confirm_server`を通す。`server_configured`は
セッションの設定から取得し、未確認の候補やQMLの独立したフラグからは決めない。
保存失敗は引き続き`settings_error`で通知し、接続フォームで完了を阻止する。

`services::Probe`はURLと取得Jobを所有し、HTTP応答の解析に成功した場合だけ
`VerifiedServer`を返す。結果の局一覧とその接続先は一つの値で運び、別のサーバーの
確認結果を組み合わせない。通常の設定確定はこの型を必要とする。
設定の`Loaded`はファイルと環境変数の値を取り込み、`activate`で実行中の`Session`に
移る。SessionはPreferencesの可変参照を公開せず、`Change` enumの操作と
`confirm_server(&VerifiedServer)`だけを受け付ける。起動時の既存設定の復元と、
実行中の新規接続先の確定を区別し、保存形式と環境変数の挙動は維持する。

番組表の表示は既存の`Guide`型を正とし、`guide_visible`を読み取り専用で公開する。
QMLの操作は`guide_open`を呼ぶだけで、表示フラグを別に書き換えない。
公開データの初期化後に表示を通知するため、Loader生成中の最初の日付要求も受け付ける。
同じ表示状態への要求では選択中の日付を消さない。日付選択部品の遅延処理は
その部品が所有するTimerで実行し、破棄とともに取り消す。

これらの不変条件とQt通知中の読み取りは`test-connection.sh`、製品のMain.qmlを含む
画面同士の接続は`test-startup.sh`で確認する。[Qtテスト](qt-tests.md)

## 起動と完全無効化

機能の許可リストはQt/GStreamerの初期化前に確定。通常起動では字幕処理・EPG取得を有効にする。
字幕の表示選択と実況の有効化は保存設定から復元し、UIで変更可能。
`--features=none` または `subtitles,epg,comments` の任意の重複しない組み合わせは
厳密な許可リストで、その実行だけに適用。
未許可の機能はUIからも起動できない。この明示的な検証モードでは保存設定を読み書きしない。
通常起動の設定互換性と保存タイミングは [feature-migration.md](feature-migration.md) を参照。

開発用起動オプションで字幕機能を除いた場合はSessionそのものが不在。字幕デコーダー、TS probe、tsdemux統計、時計同期、
字幕poll、表示Loaderを生成しない。ARIBライブラリーはバイナリーにリンクされるが
デコーダーインスタンスは作らない。「字幕を表示」OFFは解析を維持してLoaderだけ破棄する。
表示を戻すと次の字幕更新から表示する。

EPG OFFは番組通信・更新予約・スナップショット・投影データ・表示Loaderが不在。
停止処理中は通信Jobを保持し、Tokioの完了を確認してから破棄する。共有HTTPプールは残る。
番組表を閉じるだけの場合は投影とLoaderを破棄し、5分ごとの取得は継続する。

実況OFFは受信・投稿接続の停止要求と履歴・下書き・投影の破棄を行い、勢い取得も取り消す。
接続の世代管理はviewer-commentsのControllerが担当する。流れる実況は
comments_enabled・danmaku_enabled・playingが揃う間だけLoaderで生成する。
表示だけOFFの場合と機能そのもののOFFを区別する。履歴は200件、流れる項目は64件まで。
詳細な受信上限・再接続・終了契約は [comments-migration.md](comments-migration.md) を参照。
投稿は操作ごとに既存runtimeで最大1つの短いWebSocketセッションを開始し、
結果を確認して終了する。自動再送や投稿待ちキューは持たない。[投稿の契約](comment-posting.md)。

## 字幕の停止順序

選局・停止では、playback::Sessionが先にPlaybackをREADYへ遷移させ、ストリーミング
タスクが停止してから旧字幕Sessionを破棄する。成功時だけ返る`Stopped<'_>`は
この所有者を排他的に借用し、次の字幕生成と再生開始で消費される。
停止に失敗するとこの型は得られず、以前の字幕資源を保持して再試行できる。
Playerは字幕Sessionを個別に取り外せない。下位Playbackの生の開始・停止・attach・element
APIもモジュール外に公開しない。Dropは従来どおり、ネイティブ停止に成功してから字幕を解放する。
視聴者の字幕表示ON/OFFではSessionを維持し、映像の再接続は行わない。
購読Scopeはsignal/probeの解除IDを所有し、対象をWeakRefで保持して循環参照を避ける。
解除時にtsdemux統計をOFF、signal/probeを除去、bus sync handlerを解除し、時計状態を破棄。
その後に新Sessionを作り再生するので、旧局の字幕は新局に混ざらない。

現在bus sync handlerを使う任意機能は字幕だけ。将来別機能も同期メッセージを必要とするなら、
Playbackに単一ディスパッチャーを置く。機能ごとに上書きする設計には拡張しない。

## EPGの停止順序とデータ

取得状態は `Acquisition::Idle / Fetching / Cancelling` で表す。
Fetchingは結果受信可能なJob、Cancellingは終了確認専用のStoppingを所有し、
待機中だけ次回取得期限と前回の結果を持つ。Job::cancelは所有権を消費し、
結果チャネルをその場で解放する。Stoppingには結果を読むAPIがない。表示用の文字列は保持せず、
型付きStatusをPlayerでQt向けの文字列へ変換する。

Job/Stoppingの`poll(self)`は所有権を消費する。未完了なら`Progress::Pending(元の操作)`、
完了なら`Progress::Complete(結果)`を返す。結果がキューに入っていてもワーカーが
終わるまでPendingを維持し、終了したのに結果がない場合の分類もこの境界で行う。
呼び出し元は完了したJobを再取得に使えず、各機能で終了確認と結果取得の手順を重複させない。

設定変更時に古い結果の受理を止め、公開データを空にし、通信をabortする。
完了確認までは停止待ちとし、新たなJobは開始しない。A→B→Aでも古い結果を受け取らない。
HTTP失敗・サイズ超過はEPGの状態に表示し、再生を止めない。更新失敗では同じサーバーの
前回成功分を維持する。サーバー変更／無効化では空にする。

同じJob→Stoppingの遷移を局一覧・実況勢い取得にも使用する。
HTTP本文の上限付き取得、結果チャネル、Job／Stopping／Taskの所有権は
`services/job.rs`にまとめる。`services/server.rs`はURL検証と接続確認の型、`services.rs`はエラー型・共通Networkの
生成と各機能への接続を担当する。Jobの生成にはRuntime HandleとHTTP Clientを渡し、
呼び出し元のNetworkやQt状態を保持しない。既存の`services::Job`等の公開経路は再公開で維持する。
停止待ち中の再設定では既存Stoppingを保持し、最新の希望状態だけ更新する。
TaskのDropはabortを要求するが、終了を待ったことにはしない。
[Tokio JoinHandle仕様](https://docs.rs/tokio/1.53.1/tokio/task/struct.JoinHandle.html#method.is_finished)
に従い、Job/Stopping内部でis_finishedを確認してからCompleteを返し、次の取得を許可する。
取消し済みの完成データを即時解放する試験と、部分HTTP本文の切断・同一サーバーへの
再設定・無効化後の旧結果拒否の既存試験が成功。同期JSON解析中のabortは解析終了まで
待つ可能性があり、取消し要求だけで即時終了するという保証はしない。

番組JSONのHTTP応答上限32MiB、番組数50,000件、番組取得は同時1件、現行スナップショット1世代。
変更通知の購読はこれとは別接続で、60秒に集約した再取得要求を渡す。
更新中だけ受信バッファーと新候補が共存する。番組表は指定日（7日分から選択）の全局を投影する。
閉じている間は番組表のJSON投影を作らない。現在番組は1秒間隔で選び、本文は変更時だけ投影する。
EPGとの照合はチャンネルのnetworkId・serviceIdの明示メタデータを使い、配信用IDから推測しない。

字幕はPES約64KiB/ PID、字幕PID最大8、PMT PID最大32、待機128画面。
1画面最大2048セル、本文／セル文字列各128KiB。超過字幕は破棄。期限なし字幕は
次の更新またはクリアまで表示する。時刻は通常版から引き継いだPTS同期実装を使う。

## エラーの境界

通信・チャンネル・EPG・字幕セッション開始・再生はthiserrorによる型付きエラーを返す。
HTTPのエラーと機能ごとの解析エラーは `FetchError<E>` で区別し、HTTPライブラリーに
EPG固有のエラー型を依存させない。元のHTTP・JSON・GStreamerエラーはsourceとして保持する。
再生の途中再開拒否は `playback::Error::LiveResumeRejected` として分類し、復旧条件を
文字列比較や動的エラーのdowncastに依存させない。UIと診断ログで文字列へ変換する。
字幕デコーダーの生成可否しか得られない既存の境界は `DecoderUnavailable` で表し、
下位の具体的な原因を保持できるようになったと見せかけない。

EPGのparseは変換・検証のみとし、メモリー容量のログは結果を受理する箇所へ分離した。
起動引数は`features/launch.rs`へ分離し、`ParseError`で指定形式不正、オプション重複、
不正な機能名を区別する。`StartupError::Arguments`は元の型をsourceとして保持し、
終了コード2の判断は従来どおり外側の種類で行う。既存の引数の受理・拒否条件、
エラー文言、Qt/GStreamer初期化より前に解析する順序は維持する。
allocator設定の小さなエラーAPIは引き続き固定文字列で返す。

2026-09-07の起動引数整理では、既存解析テストをエラーvariantまで確認する形に更新し成功。
全ターゲットClippy警告なし、リリースビルド成功。製品バイナリーへ不正引数を5通り渡し、
いずれも終了コード2・従来と同じ標準エラー文言・標準出力なしを確認した。
記録はgit管理外の`benchmark/launch-errors/check.py`と`result.json`。
通常経路のexpectも点検し、PLANの事前初期化、静的な音声pad定義、排他的に操作する
待機キューの長さという各不変条件のコメントを確認。これらは今回変更していない。

## 診断と検証の限界

機能カウンターを10秒ごとに画面・標準エラーへ出す。資源診断の有効時は別ワーカーが
RSS・glibc使用中量・スレッド数・FD数等を採取し、操作イベントと定期sampleをJSONLへ記録する。
キュー・ファイルサイズ・保持ファイル数に上限がある。記録所有者はPlayerではなくアプリ寿命に
合わせ、QML engine終了時のGC集計を受け取ってから終了する。
詳細は [resource-diagnostics-migration.md](resource-diagnostics-migration.md) を参照。
購読数にはtsdemux統計の解除責任とbus handlerも含む。RSSを機能別に配賦するものではない。
無効化で確認するのは機能の所有物がゼロになること。Qt・ドライバー・アロケーターの
共有キャッシュまでOSへ返るとは限らないため、メモリー比較は新規プロセスごとに行う。

字幕には同梱ARIBフォント・輪郭描画、EPGには全局時間軸グリッドを移植済み。
実況、主副音声補助、ロゴ、保存設定も実装されている。ただし機能の存在は、
全画面の忠実性・実放送の音声・長時間併用の検証完了を意味しない。
仮想画面でのUI操作試験と、実GPUでのメモリー・性能検証を分ける。
短時間で増加が出ないことだけでは、長時間の増加原因を否定しない。
HTTPの途中再開拒否だけは新規接続で1回復旧する（[詳細](live-stream-errors.md)）。


## QML生成前の再生オブジェクト所有権（2026-09-07）

Qt用の映像型を登録するためPlaybackをQMLロードより先に生成する。以前は
静的OnceLockがMutex<Option<Playback>>を強く所有しており、QMLのPlayerが
生成される前に失敗するとPlaybackのDropが実行されなかった。

Preloadedを起動スコープの所有者とし、静的変数はWeakだけを保持する形に変更した。
Playerが生成されればtakeでPlaybackの所有権を受け取り、受け取らなければ
Preloadedの破棄でPlaybackも破棄される。PreloadedはQGuiApplicationより後、
QQmlApplicationEngineより前に宣言し、どの失敗経路でもQtアプリより先に破棄する。
所有者を保持する必要をmust_use属性でも示した。追加のunsafeやunwrapはない。

[QQmlApplicationEngineの仕様](https://doc.qt.io/qt-6/qqmlapplicationengine.html)で
ローカルURLの即時生成とエンジン破棄時のQMLオブジェクト破棄を確認した。
実際の失敗試験は[Qt Controlsのスタイル指定](https://doc.qt.io/qt-6/qtquickcontrols-styles.html#run-time-style-selection)
を利用し、専用Xvfb :99でQT_QUICK_CONTROLS_STYLE=StartupFailureProbeという
存在しないスタイルを指定した。音声は明示fakesink。デバイス不足を代替する試験ではない。

修正前は実行中PID 132581の/proc/exeから同じ旧バイナリーを別プロセスとして起動した。
修正前後ともQMLエラーを報告し終了コード1。GST_REFCOUNTINGログでは、
playbin3-0/qml6glsink0/deinterlace0/queue0の各finalizeが修正前0回、修正後1回だった。
Playerの生成より前の失敗で、OSのプロセス終了に任せていた要素破棄が実行されることを確認した。
これは通常再生中のメモリー増加の原因を特定した修正ではない。

通常起動も専用:99で確認し、Playerへの受け渡し後にNHK京都のPLAYING、字幕受信、
EPG取得、実況表示が動作し、閉じるボタンから終了コード0だった。最初の閉じる操作は
非表示の操作部に届かなかったため、ポインター移動の間に待ちを置いて再表示して閉じた。
Rust102件成功・3件ignored、全ターゲットClippy、書式検査、リリースビルド成功。

証跡はGit対象外のbenchmark/startup-preload-owner/にbefore.log、failure.log、
finalization-summary.json、success.logと専用stateを保存した。実GPUの
長時間観測2プロセスは停止・再起動していない。Player生成後のQML失敗は別途未確認。
