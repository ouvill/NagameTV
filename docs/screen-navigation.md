# 画面移動と戻る履歴

画面と履歴はQtに依存しないRust型が所有し、PlayerがQMLへ公開する。
設定と初回セットアップは現在の画面に重なるポップアップとし、履歴に含めない。

## QML契約

画面番号は`Home = 0`、`Watching = 1`、`Guide = 2`、`Recordings = 3`で固定する。

| 種類 | 名前・型 | 契約 |
| --- | --- | --- |
| 読取専用プロパティ | `screen_navigation: bool` | 画面移動機能が現在ONか。通知は`screen_navigation_changed()` |
| 読取専用プロパティ | `screen_navigation_allowed: bool` | この起動設定でONにできるか。起動中は不変 |
| 読取専用プロパティ | `screen: int` | 現在の画面。OFFでは`Home`を返し、従来の画面表示には使わない |
| 読取専用プロパティ | `back_screen: int` | 戻る操作後の画面。`[Home]`とOFFでは`-1` |
| 読取専用プロパティ | `watching_available: bool` | 読込中・エラー・録画終端を含む視聴セッションがあるか。通知は`watching_available_changed()` |
| invokable | `open_screen(screen: int) -> bool` | 指定画面を開く。OFF、未知の番号、EPG無効時のGuide、セッション不在時のWatchingは`false`。現在と同じ画面も受理し`true` |
| invokable | `go_back() -> bool` | 履歴の先頭へ戻る。`[Home]`とOFFでは変更せず`false` |
| invokable | `configure_screen_navigation(enabled: bool, legacy_screen: int) -> bool` | 設定を変更して保存する。OFF→ONではQMLが現在表示している画面番号を渡す。未知・利用不可の画面や起動設定で禁止されたONは`false`で変更しない。OFFへの切替と同じ値の再設定では第2引数を使わない。保存失敗は既存の`settings_error`で通知し、受理済みの実行時設定は維持する |
| signal | `screen_changed()` | `screen`と`back_screen`の通知。両方とGuideの状態を確定してから通知する |

`guide_visible`は引き続き「番組表が表示されている」を表す。
ONでは`guide_open(true)`をGuideへの移動、`guide_open(false)`を現在のGuideからの戻る操作として扱う。
Guide以外での`guide_open(false)`は何もしない。OFFでは既存の動作を維持する。

## 履歴と再生の寿命

履歴の底は常にHomeで、各画面は最大1回、全体は最大4件とする。
画面を開くと、その画面の以前の位置を取り除いて末尾に追加する。
Homeを開くと`[Home]`へ戻す。戻る操作は末尾を取り除く。
たとえば`Home → Recordings → Guide`から戻るとRecordingsになり、
`Home → Guide → Watching`から戻るとGuide、次はHomeになる。

画面移動だけでは再生を停止しない。受理した選局、ファイル・URL・録画の読込要求は
Watchingを開く。入力検証で拒否された要求は画面を変えない。
読込失敗・再生失敗・一時的な再接続でも視聴セッションを維持する。
録画の終端では再開・シークができるため、閉じるまでWatchingを維持する。
明示的な停止、サーバーの置換、正常な終了でセッションを閉じ、履歴からWatchingを取り除く。
停止に失敗して再生資源が残った場合は維持する。
チャンネル一覧が空になっても既存の再生所有者が入力を保持している間は閉じない。
ファイル読込を取り消した場合は、読込前のセッションの有無へ戻す。

Guideが現在の画面になると、予約接続の確認、チャンネル・EPG更新、`AwaitingDay`への遷移を行う。
離れると`Closed`にし、既存の`guide_release()`でモデルを解放できる。
EPGを無効にすると履歴中のGuideを取り除く。
起動時は`[Home]`とし、自動再生や起動時の動画読込が始まるとWatchingを追加する。

## フラグとQML側の切替

保存設定`screen_navigation_enabled`は既定OFF。
検証時は`--features=screen-navigation`、番組表も使う場合は
`--features=epg,screen-navigation`で有効にする。
明示的な`--features`指定では通常設定を読み書きせず、screen-navigationを含まなければ
設定画面からもONにできない。

OFFでは履歴を保持せず、既存のQML画面移動とGuideの動作を使う。
ONにすると`[Home, 現在の画面]`から始める（Homeなら1件）。
現在の画面は設定ポップアップの背後を指す。QMLだけが持つRecordingsの表示状態を
Rustが推測しないため、切替APIの第2引数で渡す。

QMLはONで`screen`を表示とナビゲーションの選択状態に使い、再生種類から推測しない。
戻る操作では`go_back()`が`false`の場合だけ既存の全画面解除を行う。
OFFへ切り替える直前に`screen`を読み取り、従来の`Main.page`をRecordingsなら録画一覧、
それ以外ならViewingへ設定する。GuideはRustが表示状態を維持する。
Homeで再生中の場合、従来画面には対応する表示がないためViewingへ戻る。

再生開始の通知をQMLから追加する必要はない。ONでは既存の`onStarted`や選局処理から
録画一覧・Guideを手動で閉じる処理を行わず、Rustが確定した`screen`に従う。
Guideのカード・日付・フォーカスなど表示部品の状態は引き続きQMLが所有する。

## 検証

履歴の重複排除、Homeへの復帰、戻る操作、セッション終了、EPG無効化を単体テストで検証する。
ハードウェア不要のPlayer接続試験でOFF時の既存動作、ONでの画面・Guideの整合性、
通知中の読み取り、実行時切替、設定保存・復元を確認する。
製品QMLの起動・操作試験はUI担当が行う。
