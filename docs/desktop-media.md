# デスクトップのメディア操作

LinuxではMPRISを使い、GNOME/KDEなどの通知領域・メディア操作欄にNagameTVの
番組名と再生状態を公開する。表示位置・体裁はデスクトップ側に依存する。
Windows／macOSのメディア連携は未実装。

- 再生・一時停止・停止・音量・ウィンドウを前面に出す操作を既存のPlayerへ配送する。
- `Rate`は適用済みの再生速度を公開し、0.5〜2.0倍・0.1刻みの変更をPlayerの共通操作へ配送する。
  ライブ位置付近では範囲を1.0に制限する。終端などで非等速を保持する間は現在値と1.0を含む範囲を公開する。
  `Rate=0`は一時停止として扱う。
- 一時停止は録画とタイムシフトで利用できる。保持なしのライブでは一時停止を公開しない。
- 番組名はアプリが再生位置に対応付けたTS番組情報を使用する。不明なら録画名／局名を使う。
  通知へサーバーURL・録画の絶対パスは渡さない。
- 録画では長さ・位置・シークを公開する。範囲が動くタイムシフトでは絶対シークを公開しない。
- 停止・再生終了では番組のメタデータを消す。ウィンドウの終了時は登録を解除する。
- ミュート中の公開音量は0。外部からの音量変更は通常のスライダーと同じくミュートを解除し保存する。

Linux専用のQt DBus境界は`rust/src/desktop_media.h`、再生状態の投影と命令の実行は
`rust/src/player/desktop_media.rs`。登録前／登録済み／利用不可をenumで分け、
登録済みの所有者だけが更新・受信できる。検出したセッションバスに登録できなければ
警告を一度記録し、メディア連携を利用不可にする。再生自体の条件は変更しない。

バス名は`org.mpris.MediaPlayer2.io.github.ouvill.nagametv.instance<PID>`、
オブジェクトパスは`/org/mpris/MediaPlayer2`。Flatpakにはこの名前の所有権のみ追加する。
状態更新は既存のPlayer pollで行い、変更のあったプロパティだけを通知する。
Positionの定期通知は行わず、シーク完了時はSeekedを送る。
要求の保留は32件、1回のpollで最大8件。遅れて届いたシークは入力の世代も再検証する。

`bash scripts/test-desktop-media.sh`は機器不要。QCoreApplicationと専用D-Busで
合成メタデータの型・プロパティ・操作・通知・不正値・古いシーク・登録解除を確認する。
`bash scripts/test-startup.sh`は検証済みの専用画面・実GPU・仮想音声を使い、
製品Main.qmlの録画再生中に外部D-Busクライアントからタイトル・一時停止／再開・音量・一時停止を維持した速度変更を確認する。
GNOME/KDEの実際の通知領域の外観、物理メディアキーはこの専用環境の確認対象外。

参照仕様:
[MPRIS Player](https://specifications.freedesktop.org/mpris/latest/Player_Interface.html)、
[MPRIS root interface](https://specifications.freedesktop.org/mpris/latest/Media_Player.html)、
[Qt QDBusVirtualObject](https://doc.qt.io/qt-6/qdbusvirtualobject.html)。


2026-09-21の検証: 専用D-Busでの機器不要テスト、Qt接続・翻訳テスト、Clippyが成功した。
専用GUI環境の製品起動試験でも、録画タイトル・一時停止／再開・音量と正常終了を確認した。
