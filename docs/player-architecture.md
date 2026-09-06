# Playerの責務と所有権

アプリケーションは、純粋な状態遷移、副作用の実行、Qt表示アダプターの3層で構成する。

| 場所 | 責務 | 所有するもの |
| --- | --- | --- |
| `rust/crates/viewer-core` | 型付きの状態、操作の判断、状態遷移、副作用の指示 | immutableなState、EPG・カタログ・字幕・履歴の共有スナップショット |
| `rust/src/runtime.rs` と `runtime/` | 通信、GStreamer、設定保存、診断 | タスク、結果キュー、再生パイプライン |
| `rust/src/player.rs` と `player/` | 操作をイベントへ変換し、状態をQtへ投影 | QObjectプロパティ、前回投影したスナップショット |
| `qml/Main.qml` | 表示、入力、画面内の選択・アニメーション | パネル開閉、ホバー、レイアウトなどの表示状態 |

```text
Qtの操作 → Event → update(&State, Event) → Transition { state, effects }
                         ↑                          ↓
                       結果Event ← RuntimeがEffectを実行
                                                    ↓
                                 Qtアダプターが確定した状態を投影
```

## immutableなアプリ本体

`viewer_core::app::State` のフィールドは非公開で、外部には共有参照を返す。
`update` は入力のStateを変更せず、新しいStateとEffectの列を返す。
関数内のローカル変数は更新するが、外から観測できる以前のスナップショットは変わらない。
EPG、カタログ、字幕、音声一覧、コメント履歴はArcで共有し、全EPGを遷移ごとにコピーしない。
履歴の追加ではcopy-on-writeを使い、200件の上限をコアで保証する。

コアの依存はserdeとURL解析のみ。Qt、GStreamer、Tokio、reqwest、ファイルIO、
システム時計を参照しない。EPGもロックを持つストアではなく、完成済みの値として構築する。
時刻はTickイベントで渡し、同じ状態とイベントから同じ遷移を再現できる。

選択中サービスはOption<u64>。現在番組はEPGと時刻から導出し、見つからない場合はNone。
タイトル、説明、開始時刻、時間、進捗を別々のアプリ状態として保存しない。
選局の判断でQStringや表示用配列を読み戻さない。

## 副作用と結果

コアはStartPlayback、LoadCatalog、ConnectComments、SaveSettingsなどを発行する。
Runtimeだけが実行し、PlaybackStarted / PlaybackFailed、CatalogLoaded、SettingsSavedなどの
結果イベントをコアに戻す。失敗時の表示と後処理の判断もコアに置く。

翻訳の適用はQtが提供する表示機能なので、RuntimeからPresentation::ApplyLanguageとして
アダプターへ渡す。アダプターはLanguageAppliedで成功・失敗を返す。
字幕設定も適用結果を受けて確定する。通常の副作用実行はGUIスレッド上で行い、
通信ワーカーだけをTokioに載せる。QObjectやQQuickItemをワーカーへ渡さない。

50msのQMLタイマーはイベントを取り出すためのポンプで、更新間隔の判断はコアにある。
チャンネル一覧を開く操作はRefreshイベントを送るだけ。60秒の抑制、5分周期の再取得、
取得中の重複防止、自動再生の一度だけの実行はRustが判断する。
QMLは最初の映像フレームの準備完了をVideoReadyとして通知する。
字幕の表示時刻は引き続き再生パイプラインの時計に合わせる。

## リクエストと寿命

`runtime/catalog_request.rs` のCatalogRequestが取得タスクと容量1件の結果キューを所有する。
破棄時にNetworkTaskがabortを要求し、キュー内の古いペイロードも解放する。
コアはリクエスト番号を検査し、受理した結果だけを新しいStateへ格納する。
A→B→Aでも古い結果を受理しない。EPG通知と実況通知にも世代番号を付ける。

サーバー変更では選択・カタログ・EPG・字幕・音声表示・コメント履歴をまとめて更新し、
旧リクエストの取消しと新しい購読をEffectとして発行する。
通信の共有可変EPGストアは置かない。
実況は受信キュー256件、1回の処理64件、履歴200件、画面上の同時表示64件に制限する。

## Qtとの境界

`player/state.rs` のPlayerRustはRuntimeと表示用フィールドを所有する。
`player/adapter.rs` は操作の変換とQt固有の映像取付け・翻訳・フォルダー表示を担当する。
`player/render.rs` が状態からプロパティを一方向に投影する。
Qt側の番組表は既存QML向けのQStringListだが、アプリ本体のデータ源には使わない。
数値から文字列への変換も表示境界に限る。

出力プロパティはQMLから読み取り専用。音量などの書き込み可能な設定プロパティは
カスタムsetterからPreferenceイベントを送る。表示値を先に変更するsetterは持たない。
一回の投影では全フィールドを更新してから変更通知を発行するため、通知先が
異なる更新世代の配列を読むことを防ぐ。変更のない値は通知しない。
番組表・履歴・字幕の変換は前回のスナップショットと比較し、必要なときだけ行う。

QObjectの更新はGUIスレッド内に限定する。パネルの開閉、ホバー、レイアウト、
翻訳、日時の表示形式、アニメーションはQtの責務として維持する。
長時間のメモリー実測には[診断ログ](passive-diagnostics.md)を利用する。

## 検証

```sh
# QtもGStreamerもビルドせず、純粋なアプリ本体だけを検証
cargo test --manifest-path rust/Cargo.toml -p viewer-core

# 既存のCPU・メモリー・ローカルHTTPテストを含めて検証
cargo test --manifest-path rust/Cargo.toml --workspace
cargo check --manifest-path rust/Cargo.toml --workspace
```

コアのテストでは旧スナップショットの保持、選局・再試行、番組の空白時間、
古い通信結果の拒否、更新抑制、自動再生、履歴上限、設定値と副作用の整合を確認する。
Runtimeの失敗経路テストはデバイスや通信を初期化せず、明示的に未初期化の依存を渡し、
副作用の失敗が結果イベントを通じて状態へ反映されることを確認する。
Qtの投影テストはQStringの値を扱うだけで、ディスプレイや再生デバイスを使わない。
実画面・実音声の視聴試験、実Mirakurunでの接続、長時間のRSS測定は別途必要。

## チャンネル切り替え

選局の判断は `viewer-core/src/selection.rs` の `SelectionAction` に分離する。
同一サービスが接続中・再生中なら `Keep` として戻り、再接続、設定保存、実況再起動、
字幕や音声のリセットを行わない。停止・エラー時は `Start` として同じ局にも再試行できる。
未選局または現在局が一覧にない場合、「次」は先頭、「前」は末尾を選ぶ。

再生開始は `playback/session.rs` の処理を通り、次の順序を守る。

1. Busをflushし、旧ストリームをREADYまで停止してからflushを解除する。
2. 旧音声ストリームの参照、字幕待ち行列、TSのPES/PSIバッファー、ARIBデコーダーを破棄する。
3. 選択した局の現在番組から取得した `AudioProgram` を設定する。
4. URIを変更し、PLAYINGへの遷移を開始する。

EPG検索は選択したサービスのスケジュールを二分探索し、その音声情報だけを複製する。
番組未取得・番組の空白時間は `None` とし、以前の局の情報を持ち越さない。
Rustへ渡すTickで番組境界を反映する。初回の音声情報設定は再生開始のEffectに含める。
リセットに失敗した場合はResultで再生開始を中断し、既存のエラー表示・停止処理へ渡す。

GStreamerの[状態遷移仕様](https://gstreamer.freedesktop.org/documentation/additional/design/states.html)では
PAUSED→READYでストリーミングスレッドを停止し、動的padを削除する。
[READY/NULLへの遷移はASYNCを返さない](https://gstreamer.freedesktop.org/documentation/gstreamer/gstelement.html#gst_element_set_state)。
そのため旧ストリーム停止後に新しい状態を設定する。Qtの映像sinkとGLコンテキストは再利用し、
通常停止・破棄時はNULLへ戻す。QObject/QQuickItemの操作は
[Qtのスレッド規則](https://doc.qt.io/qt-6/threads-qobject.html)に従いGUIスレッドに維持する。

回帰テストは選局・再試行・折り返し・EPG時間境界、再生開始前の音声情報設定、
旧音声ストリーム参照の解放、字幕状態の初期化を確認する。
ローカルHTTP配信を使った5回の反復テストでは、実際のsouphttpsrcとqueueを通して
READYによる応答受信の中断、ソケット切断、キューの空化、Bus内の旧参照解放を確認する。
このテストは明示的なテスト用fakesinkでバイト列を消費し、映像・音声デバイスを使用しない。
実Mirakurunのチューナー解放完了時間、実画面・実音声、長時間のRSS推移の測定を代替しない。

## 操作UIとマウス移動

`pointer_activity.h` の監視オブジェクトは、プレイヤー領域のQQuickItemが所有する。
`player/pointer_activity.rs` がQMLからの取付けを仲介し、ウィンドウのMouseMove/Enter/Leaveを
イベントフィルターで観測する。領域内への進入・座標の変化で `activity()` を通知し、
QMLの `reveal()` が表示と3.2秒の非表示タイマーを更新する。
同じ座標の通知はタイマーを延長しない。フィルターは常にfalseを返し、クリックやドラッグを消費しない。
ウィンドウ変更時は旧フィルターを外し、Item破棄時はQObjectの親子所有権で監視を破棄する。

Wayland上の実アプリで、ウィンドウには移動イベントが届いてもHoverHandlerが反応しないケースを
確認したため、ホバー配送や定期座標ポーリングには依存しない。
[Qtのイベントフィルター](https://doc.qt.io/qt-6/qobject.html#installEventFilter)を用い、
QObject・QQuickItem操作はGUIスレッド内に限定する。

`scripts/test-pointer-activity.sh` は実ディスプレイで、移動・再進入・クリックの通過・
ウィンドウ移動後の再接続・監視の解放を検証する。実アプリでもX11/Waylandそれぞれで、
非表示からの再表示、連続移動中の表示維持、移動停止後の非表示を確認する。


## 字幕処理の有効・無効

`transport::TransportParser` はTSのフレーミングとPAT/PMT解析を担当し、音声構成と
字幕PIDを追跡する。字幕PESの組み立て・libaribcaptionの所有権は
`subtitles::CaptionDecoder` に分離し、字幕OFF時はこの部品を破棄する。
共通解析はOFF中も継続するため、音声構成の更新は停止しない。

`subtitlesEnabled` のsetterはPreferenceイベントだけを送る。コアがSetSubtitlesを発行し、
実行部分から成功イベントを受けたときに設定値と現在字幕を更新する。失敗時は設定値を維持し、
再生エラーへ遷移する。Qtは更新後の状態を表示へ反映する。
起動時に保存値を適用し、選局・停止のストリームリセットでも有効状態を維持する。
受信側は抽出器のロックを字幕キュー投入まで保持し、OFF側も同じロックの下で
デコーダを破棄して時計・待ちキューを解放する。これによりOFF前の字幕の遅延投入を防ぐ。

ON時は新しいデコーダで次の字幕PES開始から再開し、映像との時刻対応を再取得する。
字幕管理情報も作り直すため、放送側から必要な情報が届くまで字幕が出ない場合がある。
OFF中もGStreamerの統計通知と映像pad監視は残るが、字幕用の時刻履歴・字幕を蓄積しない。
