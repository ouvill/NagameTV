# 音声トラック選択の移植

mainのrust/src/audio.rsとqml/Main.qmlのaudioSettingsを参照する。
ネイティブトラック選択、PMTとTS番組情報の照合、言語・主音声／副音声の表示、
二重音声の片側出力と3モード選択、主音声の初期選択を実装した。
実放送の二重音声・長時間の検証も残り、音声機能全体の互換完了ではない。
以下の途中段階の検証記録にある未実装の記述は、その段階の状態を表す。
現在の二重音声実装は末尾の節を参照する。

## 現在の情報源（2026-09-19）

音声の主／副・言語・二重音声は、共通TS入力が収集したEIT p/fの
audio_component_descriptor（0xc4）を使う。解析するフィールドは
component_tag、component_type、main_component_flag、言語コードで、
[ARIB STD-B10 Part 2, 6.2.26](https://www.arib.or.jp/english/html/overview/doc/6-STD-B10v5_13-E1.pdf)
に従う。GStreamerの音声カタログとPMTのPID/tag照合は引き続き利用する。

`Session::audio_metadata`が再生出力の位置で既存TSカタログを引き、録画・ライブ・
タイムシフトの音声選択へ同じ型を渡す。PCの現在時刻、選択中チャンネルの一覧、
Mirakurun EPGは音声判定に使わない。開始日時・放送時計が未取得でも、
TSのpresentイベントと音声記述子が得られれば判定できる。
音声のためTS番組解析は常時有効にし、EPG設定は番組通信・表示の有効化を制御する。

選択キーには入力の同一性、放送サービス、TS ID、event_id、任意の開始日時と
音声記述子を含める。シーク中は位置未確定として片側出力を解除し、
次の音声選択は確定した再生位置で再検証する。メニューを閉じていても照合する。
同一番組内の音声形式変更は観測位置に属し、番組名の後日訂正と一緒に
過去の位置へ遡って適用しない。欠落・重複タグから主／副を推測しない。
音声情報を番組表示用JSONへ含めず、TS履歴のメモリー上限計算には保持量を加算する。

検証: 機器不要のRust試験254件成功・外部入力等の3件はignored。
Clippy指摘修正後の音声関連21件も成功した。生成したCRC付きEITから
音声記述子・番組カタログを通し、時計なしの判定、前後移動、同一番組内の
形式変更、欠落・重複・途中切断と別入力の選択キーを確認した。
Qt接続試験と製品Main.qml試験（録画の時計/PID変更、メモリー/ファイル方式の
タイムシフト、初回・設定復元・自動再生・終了）も成功した。
専用GUI環境で音声メニューのQt試験5件（初期化・終了を含む）が成功し、
`cmake --build build`で通常の配布用バイナリーを生成した。

厳格な全ターゲットClippyには、変更対象外の`screenshot_native.rs`に既存の
`collapsible_if`が2件残る。同lintだけをコマンド引数で許可した全ターゲット検査は成功。
ソース側の警告抑制は追加していない。書式・差分検査も成功。
実放送の二重音声の聴取や、物理音声機器の確認は行っていない。

以下のMirakurun EPGとの照合に関する記述は移植途中の履歴である。

## 責務と寿命

playback/audio_streams.rsは現在のStreamCollectionを1つだけ保持し、選択済みのIDと
ユーザーが要求したIDを分ける。古いコレクションは置換時に解放する。
要求には配列indexを使わず、現在のコレクションで音声IDが実在することを確認する。
選択イベントには選択中の非音声ストリームも含める。初回確認前なら最初の映像を残す。
イベント送信成功だけではselectedを書き換えず、StreamsSelected通知で確定する。
番組中のコレクション変更時は要求を再送し、対象が消えた場合は要求を破棄して型付きErrorを返す。

Playbackの既存bus pollから通知を受ける。専用のスレッド・Timer・通信は追加しない。
停止に成功した後でコレクション・選択・要求を破棄し、次の局へ持ち越さない。
停止失敗時は再生状態が確定していないため、状態を先に消して成功を装わない。

player/audio_streams.rsがJSONとQStringの境界を担当する。
音声メニューの表示中に限り1秒間隔で小さなトラック一覧を投影する。
JSONが同じならQMLのプロパティ変更は生じず、閉じるとJSON・行モデルを空にする。
ストリームのタグは言語コード・タイトルのみ参照する。EPG全体やデコード済み音声を複製しない。
トラック順から主／副や言語を推測しない。

ユーザーの切り替え失敗はメニュー内に表示し、映像の停止エラーとは分離する。
コレクション更新時の自動再適用失敗もOption<Error>で最新1件を保持し、メニューへ渡す。
次の要求成功または再生停止で消す。メニューを開き直しても、未解決の失敗を再取得する。
エラー文字列や履歴は蓄積せず、QString化は表示を更新する境界だけで行う。

## UI

AudioSettings.qmlはmainと同じ左下x=24、下部から100px、最大380×340px、
padding20、角丸18、背景#f21a1c1a、選択色#389caf9fを使う。
音量ボタン横の28pxのchevron-downから開く。Escapeと外側クリックで閉じる。
1トラックまたは停止中は選択操作を無効化し、エラーと放送由来の文字列はPlainTextで表示する。
主／副メタデータ未統合のため、ラベルは音声番号と取得できたネイティブの言語コード・タイトル。

参照した公式API:
- [playbin3](https://gstreamer.freedesktop.org/documentation/playback/playbin3.html)：StreamCollectionとSelectStreams、再生中のコレクション更新。
- [Stream selection](https://gstreamer.freedesktop.org/documentation/additional/design/stream-selection.html)：選択要求は維持する映像IDも含める。
- [Qt Popup](https://doc.qt.io/qt-6.8/qml-qtquick-controls-popup.html)：opened、closed、Overlay、closePolicy。

## 検証

Rust56件成功・外部TS依存1件未実行、Clippy全ターゲット成功。
Qt62件成功（初期化・終了を含む）、AudioSettingsのqmllint警告なし、releaseビルド成功。
Rust試験は映像／テキストを保持する選択イベントの実送信、選択通知前後の状態、
消滅・非音声IDの拒否、初回の映像保持、古いコレクションの弱参照による解放確認を含む。
ネイティブイベント試験は明示したCPU用fakesinkでイベントのみを受け、再生機器を使用しない。
Qt試験は行からの実ID配送、要求だけでは選択状態を変えないこと、停止中の無効化、
PlainText、閉じた後のモデル解放と更新停止を確認する。

DISPLAY・NVIDIA OpenGL・PulseAudioを検出・検証後、実放送のPLAYING、メニュー内の
音声1トラックと選択色、正常終了を確認した。証跡はGit対象外の
benchmark/viewing-design/audio-menu.py、audio-menu.log、audio-menu.png。
今回の放送は1トラックだったため、複数トラック間の実音声切り替えは未確認。
二重音声、番組変更との併用、長時間の資源測定、mainとの同一データ画像比較は残る。

## ネイティブの複数音声検証と次の統合

GStreamer公式の[testsrcbin](https://gstreamer.freedesktop.org/documentation/debugutilsbad/testsrcbin.html)
を使い、振幅0.1と0.8の2音声と小さな映像をplaybin3へ渡すCPU試験を追加した。
音声はF32LEとして試験用sinkのhandoffで測定する。ストリーム順を仮定せず、最初の振幅を
基準として、別トラックと元のトラックの往復後にそれぞれの振幅範囲へ戻ることを検証する。
選択通知だけで成功とせず、音声バッファーと映像フレームが両方とも進むことを要求する。
条件の待機は5秒上限で、失敗時もRAIIでNULLへ遷移する。これは生成メディアの結合試験であり、
実放送の複数音声・音声出力機器・二重音声の検証とは区別する。

主／副の統合では、字幕のsource probeを常駐させる案より先に、既存tsdemuxがbusへ送る
PMTの利用を検証する。[GstMpegtsSection](https://gstreamer.freedesktop.org/documentation/mpegts/gstmpegtssection.html)
はElementメッセージからPMTとPID／descriptorを取得するAPIを提供する。
稼働環境と同じ[1.28.2のmpegtsbase.c](https://github.com/GStreamer/gstreamer/blob/1.28.2/subprojects/gst-plugins-bad/gst/mpegtsdemux/mpegtsbase.c)
ではPMT適用後、壊れていないsectionをbusへ投稿する経路を確認した。
これにより追加のTSバッファー・ストリームスレッドのMutexを避けられる可能性があるが、
実サービスのcomponent_tagを取得できることはまだ未検証。依存導入と実PMTの確認が次の段階。
調査時は開発用pcファイルがなかったため、後述の実装時に開発パッケージを導入した。

番組側はmainのaudio_programと同じ現在番組を参照し、serviceId・component_tagで照合する。
字幕Sessionに残る配信用idからのserviceId剰余計算を流用しない。
PMT更新・選局・EPG無効・メタデータ欠落で主／副の推測を残さない設計が必要。

今回の検証はRust全体57件成功・外部TS依存1件未実行、Clippy全ターゲット成功、
音声メニューのQt試験3件成功、qmllint警告なし。Clippy指摘で試験のサンプル読み取りを
as_chunks::<4>()へ修正した後、該当する結合試験を再実行して成功した。

修正後のreleaseビルド成功。表示・NVIDIA OpenGL・PulseAudioの再検証後、実放送の
PLAYING・音声メニューの表示・正常終了を確認した。新しいaudio_error呼び出しを含め
QML実行エラーは検出されなかった。証跡はGit対象外のaudio-recovery.logとaudio-recovery.png
（benchmark/viewing-design内）。実放送での自動再選択失敗そのものの再現は未実施。

## PMT bus通知によるcomponent_tagの取得

playback/audio_components.rsを追加し、gstreamer-mpegts 0.25.2の安全なSection／message APIを使う。
Ubuntuのlibgstreamer-plugins-bad1.0-devを導入し、pkg-configで1.28.2を確認した。
RustバインディングはPMTの高水準デコードをまだ公開していないため、Section::dataで得た
バイト列からPIDとstream_identifier_descriptor (0x52)だけを読み取る。
TSの再フレーミング、PAT探索、source probe、追加スレッド・Mutexは導入しない。
アプリ側の新規unsafeやunwrapもない。

Playback::playは配信用idと別にBroadcastServiceを受け取り、明示されたserviceIdだけを
PMT照合に用いる。停止成功時に対応表を破棄する。選択サービス不明・別サービスのPMTは無視する。
既存bus pollでElementメッセージ内のsectionだけを処理し、最大1024バイト・CRC・単一section・
記述子長・PIDとtagの重複を検証する。将来用のcurrent_next=0は現在の対応表を上書きしない。
対象サービスの不正な更新では対応表を消す。完全な新しいPMTは表全体を置き換える。

保持するのは選択サービスのPID/tag一覧だけ。フレームごとの処理やEPGデータの増加はない。
一時のVecとHashSetはsection検証時のみ生成し、同じ対応表なら保持領域を置換しない。
変更時だけAUDIO_PMTログへ対応表を記録する。
音声トラックの表示用JSONにcomponent_tagを付けるが、UIへ技術情報としては表示しない。
GStreamer 1.28.2のmpegtsbase.cのストリームID生成（%s/%08x）を確認し、
8桁の16進PIDと対応する場合だけタグを返す。

CPU試験では実際のGstMpegtsSection／Elementメッセージからの取り出し、順序の異なるPID、
別サービス、将来用PMT、更新・消滅、CRC既知ベクター、全途中切断、記述子長不正、
重複PID/tagと複数sectionの拒否を検証した。Rust59件成功・外部TS依存1件未実行、
Clippy全ターゲット・releaseビルド成功。フィールド検査とCRCベクター追加後の対象2件も成功。

実放送ではサービスAPIのserviceId=41984とPMTの値が一致し、PID272/tag16を含む対応表を
取得できた。最初の試験スクリプトは期待するserviceIdを1024と固定して失敗したため、
サービスAPIから取得するよう修正した。アプリはこの試行でもPLAYING・正常終了に成功していた。

現在番組のaudiosとの照合、主／副／主副の型とPCMルーティング、メタデータ更新時の
選択解除・再適用は未実装。component_tagの取得だけで音声機能全体の互換完了とは扱わない。

修正した試験では--features=none（字幕・EPGともOFF）で実放送を再生し、
サービスAPIと一致するPMT、音声メニューの表示、正常終了を確認した。EPG_MEMORYログなし、
字幕購読0・EPGタスク0も確認。証跡はGit対象外のbenchmark/viewing-design/audio-pmt.py、
audio-pmt-isolated.log、audio-pmt-isolated.png。主／副の表示・音声ルーティングは未検証。

## 現在番組との照合と言語・役割表示

Mirakurunの[ProgramAudio定義](https://github.com/Chinachu/Mirakurun/blob/master/api.d.ts)と
mainのAudioStreams::options、audioTrackLabel、日本語翻訳を参照した。
audio.rsにDescriptor、Kind、Roleを置き、Qt・GStreamer・時刻に依存しない判定へ分離する。
component_tagが一意に一致した場合だけisMain、componentType、langsを使う。
同じタグが重複していれば、並び順で選ばずUnknownとする。

EPGのProgramがaudiosをBox<[Descriptor]>として保持し、各langsもBox<[String]>とする。
ProgramInfo::audio_descriptorsは既存Snapshotの現在番組からスライスを返す。
別のEPG取得、音声専用Snapshot、番組単位のキャッシュやTimerは追加しない。
番組表のJSONではaudiosをskip_serializingし、QMLへ全番組の音声情報を渡さない。

メニュー更新時に実再生中サービスの明示メタデータとUTC時刻を照合する。
EPG無効、無効な時計、番組終了、サービス不一致では空のdescriptorを使う。
毎回ネイティブトラックから表示を作るため、以前の役割・言語を保持しない。
役割不明ならネイティブの言語タグへ戻る。通常の音声は言語＋主音声／副音声、
二重音声は現時点の両方出力を「主／副」と表示する。二重音声の片側選択はまだ生成しない。
同じ表示名の複数トラックだけ音声番号を加える。表示位置・寸法・配色は既存メニューを保つ。

Rust62件成功・外部TS依存1件未実行、Clippy成功、音声UI試験4件成功、qmllint警告なし、
releaseビルド成功。タグ順序、重複、欠落、主／副／二重音声の判定、別サービス、
番組境界・終了、EPG無効化、番組表JSONからの除外とmain相当のラベルを検証した。
実放送でPMTのタグとEPGを照合し「日本語 · 主音声」を表示、PLAYING・正常終了を確認した。
証跡はGit対象外のbenchmark/viewing-design/audio-metadata.log、audio-metadata.png。

同じ14,865番組・同じHTTP本文10,130,531バイトの前後で容量を比較した。
以前のSnapshotは2,966,093バイト。追加後は3,725,931バイトで、増分759,838バイト（約0.72MiB）。
内訳はProgram配列容量の増分262,144バイトと音声用ヒープ497,694バイト。
EPG_MEMORYにaudio_heap_bytesを加え、文字列・配列の容量を含めて記録する。
これはアロケーターの管理領域・断片化・Qt/GStreamerを含まないデータ構造の容量であり、
RSSや長時間のメモリー安定性の検証とは区別する。

次の段階は二重音声の型付きモード・PCMルーティング・主音声の初期選択。
番組変更／PMT変更で以前の片側選択が残らないこと、実放送での二重音声の確認は残る。


## 二重音声の3モードとPCM出力

`audio_choices.rs`が現在番組とPMTの一意な照合から主・副・主／副の選択肢を作る。
Qtは選択肢の表示と要求の配送を担当し、ネイティブトラックID・番組の同一性・記述子・
Modeを含む不透明な文字列キーを返す。クリック時に現時点の選択肢を作り直して照合するため、
古い番組やPMTに対する要求をそのまま適用しない。音声番号は展開した行番号でなく
元のネイティブトラック番号を使い、mainのメニューの位置・寸法・配色を維持する。

`audio_routing.rs`のModeとFormatは音声形式の世代と選択状態を1つのAtomicU64で管理する。
`audio_routing/element.rs`はGStreamer BaseTransformのAlwaysInPlaceを実装し、
playbin3のaudio-filterへaudioconvertとともに接続する。F32LE・interleavedの
2チャンネルでだけ片側選択を認め、主は左を両側へ、副は右を両側へビット単位でコピーする。
主／副と非2チャンネルでは書き込みmapを行わない。PTS・durationは書き換えない。
BaseTransformの書き込み可能なバッファーを使い、共有メモリーのmapはcopy-on-writeに従う。
フィルター自体の変換やバッファーヘッダーの処理コストがゼロという保証ではない。

制御側は実デコード中のstream IDも確認する。Mutexはstreamイベント・選択操作でだけ取得し、
バッファー処理ではatomicの読み取りだけを行う。要求は最新1件、確認待ちは5秒上限で、
選択済みのトラックへの主／副変更では不要なSelectStreamsを送らない。
適用後も既存pollから番組・記述子・形式の世代を照合するため、メニューを閉じていても
変更を検知して片側選択を解除する。要求がない場合はこの追加の番組照合を行わない。

StreamStart・CAPS更新・flushは前の片側選択を無効にする。flushはCAPSとStreamStartの
再送を要求しないので、形式とstream IDを保持して選択の世代だけを更新する。
停止成功と要素のstopでは形式も破棄する。停止失敗を成功として扱う変更はない。
エラーはthiserrorの型にし、履歴を蓄積しない。追加のunsafe・unwrapはない。
PadTemplate生成のexpectは固定名・方向・capsの契約と、クラス初期化がResultを
返せない理由をその箇所に記載している。

参照した公式資料:
- [playbin3](https://gstreamer.freedesktop.org/documentation/playback/playbin3.html): audio-filter。
- [Pipeline manipulation](https://gstreamer.freedesktop.org/documentation/application-development/advanced/pipeline-manipulation.html): データ変換用の要素。
- [GstBuffer](https://gstreamer.freedesktop.org/documentation/gstreamer/gstbuffer.html): writable mapと共有メモリー。
- [Events](https://gstreamer.freedesktop.org/documentation/additional/design/events.html): flush・CAPS・stickyイベントの寿命。

検証は生成したPCMを使う明示的なCPU試験として、appsrc→実フィルター→fakesinkの
出力バイト列、共有入力の不変性、時刻の維持、古い世代の要求拒否を確認する。
CAPS・StreamStartを再送しないflush後も再選択して出力できることを確認した。
既存のplaybin3複数音声試験にも同じフィルターを組み込み、音声切り替えと映像継続を確認する。
Qt試験は3行の要求キー配送・無効な形式の操作禁止・要求だけでは選択表示を変えないことを確認する。

通常放送では追加フィルターを通したPLAYINGログを確認した
（Git対象外のbenchmark/viewing-design/dual-routing-normal.log）。
J SPORTS2はPMT取得後にPLAYING待ちが終了したが、ユーザーは有料放送を未契約のため、
二重音声の実放送検証には使わない。この結果だけでフィルター異常とも復号異常とも断定しない。
ユーザーが挙げた06:30のNHK BS「ワールドニュース」を実放送確認の候補とする。
実放送で主・副・主／副の音と表示が一致すること、番組切り替わり、長時間の資源使用は未確認。


今回の確認結果: Rust全体66件成功・外部TS依存1件未実行。flush回帰試験を加えた後も
該当するネイティブ変換試験が成功。Clippy全ターゲット・fmt・qmllint・releaseビルド成功。
Qtの音声UI試験はX11接続を明示して5件成功（初期化・終了を含む）。既定のWayland接続では
ウィンドウがexposeされず中断したため、その試行を成功件数には含めない。
DISPLAY・NVIDIA OpenGL・PulseAudioの検出と動作確認後、最新ビルドで通常放送の
PLAYING・実映像・正常終了を確認した。証跡はGit対象外のdual-routing-verified.log/png。
この画像では音声メニューが開いていないため、実アプリのメニュー操作成功の証拠にはしない。


## 主音声の初期選択

mainのPlayback::audio_stateとAudioStreams::optionsを照合し、isMainに一致した
通常トラック、またはisMainな二重音声のMainを初期候補にする。メタデータが不明なら
先頭を主音声と推測しない。候補が無効な形式なら情報が揃うまで待つ。
`audio_default.rs`にWaiting/User/Automaticの状態を置き、保持する選択キーは最新1件だけ。
手動で選んだ副音声・主／副のキーが現在の選択肢に存在する間、自動選択で上書きしない。
番組・対象記述子・stream IDが変わってキーが消えたら初期候補を再評価する。
他トラックだけの記述子更新では、まだ有効な手動選択を解除しない。

既存pollに1秒間隔の期限判定を追加し、メニューを一度も開かなくても選択する。
期限前と停止中は初期選択用の番組取得・一覧生成を行わず、新しいTimer・スレッド・
EPGキャッシュを作らない。1秒ごとの一覧生成には少数の文字列割り当てがあり、
フレームごとの処理ではない。停止成功時に選択と期限を破棄する。

初期要求も手動と同じキーの再検証・ネイティブ確認・PCM適用経路を通す。
拒否された自動要求を同一キーに毎秒再送せず、エラーを残して手動再試行を許す。
`AUDIO_DEFAULT requested=true`は要求処理成功を表し、音声出力確認とは区別する。
選択表示は引き続きStreamsSelectedと実ルーティング状態から作る。
[公式のstream selection仕様](https://gstreamer.freedesktop.org/documentation/additional/design/stream-selection.html)
に従い、要求イベントと選択通知を分離する。

CPU試験ではメタデータ待ち、二重音声のMainのみが初期候補になること、無効な候補の待機、
手動Sub/Bothの維持、同一自動要求の抑止、番組変更と1秒期限を確認する。
実放送の二重音声初期出力と手動切り替えの確認は引き続き残る。


初期選択追加後はRust全体67件成功・外部TS依存1件未実行。無効候補と手動選択の維持条件を
追加した後の対象試験も成功。Clippy全ターゲット、fmt、releaseビルドが成功した。
表示・NVIDIA OpenGL・PulseAudioを再検証し、X11接続を明示した実アプリで通常放送を再生。
メニューを開かずにAUDIO_DEFAULT要求が1回だけ発生し、その後5秒間に繰り返さないことと
PLAYING、エラーなし、正常終了を確認した。証跡はGit対象外の
benchmark/viewing-design/audio-default.log。これは単一音声の通常放送であり、
二重音声のMain出力や副音声との切り替え成功を示すものではない。

音声選択の4種類の失敗案内をQt境界で翻訳原文に対応付け、AudioSettingsで再翻訳する経路を追加した。既存の選択・確定・コレクション解放のCPU試験3件が成功。実放送・実画面検証は残る。詳細はlocalization-migration.md参照。


## 並列CPU回帰試験でのflush修正

2026-09-07、アプリのRustテスト95件をまとめて実行した際、二重音声フィルターの試験が
Timeoutとなった。単独では成功し、並列再試行で同じTimeoutを再現した。
試験はappsrcのpadから直接FlushStart/FlushStopを送っていた。
[GStreamerのappsrc実装](https://raw.githubusercontent.com/GStreamer/gstreamer/main/subprojects/gst-plugins-base/gst-libs/gst/app/gstappsrc.c)
ではsend_eventがFlushStop時に内部キューを処理して親へ渡すため、試験をsource.send_eventへ
変更した。送信元を迂回したflushと送信タスクの競合が原因候補であり、本体の音声ルーティングを
変更したものではない。CAPS/STREAM_STARTを再送せず、flushで手動選択を解除する契約は維持する。

修正後、全テストバイナリーを並列・nocaptureで20回、通常キャプチャで20回実行し、
各回93成功・2ignoredで終了した。外部TS試験と任意の設定保存測定はignored。
実際のGPU/音声出力を使わず、生成PCM・映像と同梱TSをCPUの検査用sinkで検証する構成。
各回は12秒の外側の監視期限を設け、修正後は期限超過なし。Clippy・fmt・diff検査も成功。

最初の全体実行では字幕のmaps_real_demuxed_pes_to_the_video_segmentも60秒以上終了せず、
テストプロセスを明示終了した。gdb attachは環境側で拒否され、スタックは取得できなかった。
その字幕試験は単独と上記40回では再現せず、原因は未確定のまま残す。
音声試験の修正によって字幕の終了待ちまで直ったとは判断しない。


字幕の終了待ちを追跡するため、同試験の終了処理を実装側の契約に合わせて
READY → callbacks解除 → bus同期handler解除 → clock無効化 → NULLと明示した。
動的padのリンク失敗はコールバック内のpanicではなく、上限1件の通知を通して
試験側のResultへ返す。試験の初期化・サンプル検査にもResultを使う。
本体の再生・字幕処理は変更していない。この変更を原因修正とは扱わない。

`SUBTITLE_TEST_TRACE=1`を指定すると、開始からNULL停止完了までの7段階を
libtestのキャプチャを迂回してstderrへ出す。stdoutと混ぜると並列テストの
進捗文字列が行内へ入るため、調査時には別ファイルへ保存する。

```sh
SUBTITLE_TEST_TRACE=1 CARGO_TARGET_DIR=build/cargo cargo test --locked \
  --manifest-path rust/Cargo.toml > /tmp/viewer-tests.log 2> /tmp/viewer-tests.stderr
```

変更後、全テストバイナリーを通常の並列設定で20回実行し、毎回93成功・2ignored、
7段階すべての記録を確認した。各回の外側の監視期限15秒を超えた実行はなかった。
記録は `/tmp/viewer-subtitle-trace-{0..19}.log` と同名の `.stderr`（一時ファイル）。
Clippy全ターゲット・fmtも成功。今回も元の終了待ちは再現せず、原因は未確定。
実放送の字幕・二か国語・GPU/音声出力の検証を代替する結果ではない。
