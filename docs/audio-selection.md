# 音声トラック選択の移植

mainのrust/src/audio.rsとqml/Main.qmlのaudioSettingsを参照する。
ネイティブトラックの選択を実装した段階であり、PMT・番組情報の照合による主／副／主副、
言語の表示名、主音声の初期選択は未移植。音声機能全体の互換完了ではない。

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
