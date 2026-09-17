# 音量とミュート

## 音声出力の選択

mainのrust/src/playback.rsと照合し、出力未指定時のPulse固定を修正した。
MIRAKURUN_AUDIO_SINK未指定かつPULSE_SERVERが存在する場合はPulse、
どちらも存在しない場合はAutomaticを選ぶ。PULSE_SERVERはmain同様に存在を判定し、
空文字や非Unicodeでも指定ありとする。明示的なpulsesink/fakesink指定は優先する。
不正値・非UnicodeのMIRAKURUN_AUDIO_SINKは引き続き型付きエラーとなる。

Automaticは要素を追加せず、Result<Option<Element>>のNoneとして表現し、
playbin3のaudio-sinkを未設定に保つ。
[GStreamerの公式仕様](https://gstreamer.freedesktop.org/documentation/playback/playbin.html#advanced-usage-specifying-the-audio-and-video-sink)
に従って標準の出力検出へ委ねる。PulseとTestDiscardは従来の要素・設定を保持し、
新しいスレッドや購読、再試行機構は追加しない。

選択規則と明示出力の要素・sync・enable-last-sample設定をデバイス非使用のテストで確認。
Automaticは要素を生成しないことを確認した。これは自動検出で選ばれる実デバイスの
検証ではない。実音声試験では利用可能な音声経路を事前に検出・検証し、
利用できない場合は停止する。GUI試験では[専用テスト環境](gui-test-environment.md)の
仮想PulseAudio出力へ`pulsesink`で接続する。CPU音声試験では`fakesink`を明示する。
どちらも実スピーカーの動作や聴取品質の成功判定には使わない。

2026-09-07: 対象テスト1件、全ターゲットClippy（警告をエラー扱い）、
書式検査、CMakeリリースビルド成功。自動選択による実音声出力は未検証。

## 音量状態

mainのQML（audioMutedボタン、音量スライダーのonMoved）とviewer-coreの出力状態を参照した。
音量を保持して消音し、解除するとその音量へ戻す。ユーザーがスライダーを動かすと解除する。
ミュートはmainと同じセッション内の状態で、設定ファイルには音量だけを保存する。
新規起動では保存音量を復元し、ミュートは解除されている。

`playback/audio_output.rs` のOutput enumはAudible(Volume)／Muted(Volume)を表す。
Volumeは有限・範囲内の値だけを保持する既存型。ユーザー操作は新しい値を返す関数とし、
NaN・無限大の音量入力は拒否する。消音中も元の音量は失わない。

`player/audio_output.rs` が操作を受け、設定・出力状態・Qt表示へ反映する。
volume_levelとaudio_mutedはQt向けの投影で、apply_audio_outputでまとめて更新する。
再生・停止・選局で出力状態を初期化しない。

main内部は消音時に実出力音量0を適用するが、こちらはGStreamerの
[playbin muteプロパティ](https://gstreamer.freedesktop.org/documentation/playback/playbin.html#playbin:mute)
を利用する。既存のsoft-volumeフラグを有効にしたまま、volumeとmuteを独立して設定する。
音量変更による消音解除では、旧音量が一時的に出ないようvolumeを先に設定してから解除する。
要素、ストリーム、タイマー、スレッド、履歴バッファを追加しない。

## 検証

Rustで消音・解除・再度の消音、音量変更による解除、範囲外入力の正規化、非有限値の拒否を確認。
実際のplaybin3でvolumeとmuteプロパティを読み、READY／NULL遷移で保持されることを確認した。
このテストはURIもPLAYING遷移もなく、音声・表示・GPUデバイスを使用しない。
実際の音声サンプルの無音化やUIクリックを検証するものではない。

2026-09-07: Rust 42件成功（外部TSが必要な1件は未実行）、Clippy警告なし、CMakeビルド成功。
実再生中のミュート・選局・停止再開を組み合わせた音声確認は未実施。

## 生成音声の出力サンプル検証（2026-09-07）

既存のplaybin3音声トラック切り替え試験を拡張し、製品と同じsoft-volumeを含むflagsと
Output::applyを使って、ネイティブ出力の振幅を確認した。振幅の異なる2音声と動画を
testbinで生成し、音声・映像とも明示したfakesinkで受け取るCPU試験である。
[GStreamerの出力観測仕様](https://gstreamer.freedesktop.org/documentation/playback/playbin.html#advanced-usage-specifying-the-audio-and-video-sink)
に従い、ストリーミングスレッドのhandoffでは最大振幅と受信件数だけをatomicに保持する。

音声を往復切り替えした後、現在のトラックをミュートして最大振幅0を確認した。
ミュートしたまま別トラックへ切り替えても0を確認し、音量を25%へ変更すると
ミュートが解除され、選択先の振幅が元の1/4に相当する範囲へ戻ることを確認した。
各段階で新しい音声・映像バッファの到着を待ち、古い観測値だけで成功としない。
途中エラーはResultで返し、終了時はガードがNULLへの遷移を要求する。

対象のネイティブ統合試験成功。音声デバイス・画面・GPUを使用せず、実放送の聴取や
Qtの音量操作、停止再開後の実音声を検証したものではない。製品処理の変更はない。
