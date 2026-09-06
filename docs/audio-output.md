# 音量とミュート

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
