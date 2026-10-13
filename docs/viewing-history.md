# お気に入り局・視聴履歴・録画の再開

お気に入りと視聴状況はQtに依存しないRustの状態として保持する。
QObjectは操作の受付と読み取り専用モデルへの投影を担当する。
以下のQML公開APIを画面実装との契約とする。

## QML公開API

プロパティとinvokableは既存のPlayerと同じsnake_case、モデルのroleはcamelCaseを使う。
IDはJavaScriptの整数精度に依存しない文字列で渡す。時間の単位はミリ秒。
Playerが返すモデルのポインターは寿命中一定で、内容をモデル通知で更新する。

### Player

| 種別 | 名前・型 | 契約 |
| --- | --- | --- |
| 読取専用プロパティ | `viewing_history_ready: bool` | 初期読込完了後にtrue。永続化できない場合もセッション内の操作は可能 |
| 読取専用プロパティ | `viewing_history_error: string` | 読込・保存の失敗。正常時は空文字列 |
| 読取専用プロパティ | `recent_channels: ChannelModel*` | 最近視聴した局。新しい順、最大20件。現在の局一覧に存在する局のみ |
| 読取専用プロパティ | `continue_recordings: ContinueWatchingModel*` | 途中まで再生した録画。更新が新しい順、最大100件 |
| 読取専用プロパティ | `previous_channel_available: bool` | 現在のライブ局と異なる、戻れる局がある |
| invokable | `toggle_favorite(service_id: string): bool` | 現在の局一覧のサービスIDを登録・解除。受付時true。不明ID・初期読込中はfalse |
| invokable | `move_favorite(service_id: string, position: int): bool` | お気に入りの0始まりの位置へ移動。範囲外・未登録・初期読込中はfalse |
| invokable | `previous_channel(): bool` | 直前のライブ局を通常の選局処理で開く。対象なしはfalse |
| invokable | `resume_recording(recording_key: string): bool` | 再開一覧の安定キーで録画を開く。受付時true、完了は既存の`recording_opened(bool)`で通知 |
| シグナル | `viewing_history_changed()` | ready/error/previous_channel_availableのNOTIFY。通知前に全状態を反映 |

`recent_channels`と`continue_recordings`はCONSTANTプロパティ。
その他の上記プロパティは`viewing_history_changed`を共有する。
操作はQt所有スレッドで受け、永続化のI/Oは専用ワーカーで実行する。
保存失敗は`viewing_history_error`とstderrのtracingへ報告する。

### ChannelModel / ChannelFilterModel

既存の`channelIndex: int`、`label: string`、`band: string`、`logo: string`、
`serviceId: string`を維持し、次のroleを追加する。

| role | 型 | 意味 |
| --- | --- | --- |
| `favorite` | bool | お気に入り登録済み |
| `favoriteOrder` | int | 登録順または並べ替え後の0始まりの位置。未登録は-1 |

`Player.channels`と`Player.recent_channels`は同じroleを公開する。
recent_channelsの`channelIndex`は元の局一覧での位置であり、履歴内の行番号ではない。
既存の`count: int`、`revision: uint`、`changed()`、`row(int): QVariantMap`も利用できる。
モデルの内容変更ではリセット通知と`changed()`を発行し、revisionを進める。

`ChannelFilterModel.favorites_only: bool`（読書可、既定false、
`favorites_onlyChanged`通知）を追加する。trueでは既存の放送波・可視条件とANDで
絞り込み、favoriteOrder順に並べる。0件なら空モデルとする。
登録時は末尾に追加し、解除後に再登録すると末尾になる。

### RecordingModel

既存の録画一覧に次のroleを追加する。

| role | 型 | 意味 |
| --- | --- | --- |
| `progress` | real | 0..1。履歴なし・長さ不明は0 |
| `watched` | bool | 終端付近まで視聴した録画 |

複数ファイルがある録画では、最後に視聴したファイルの状態を表示する。
変更はモデル通知と既存の`changed()`で伝える。
既存の`play_epgstation_file`、`open_recording`も保存位置から自動再開する。

### ContinueWatchingModel

`count: int`（読取専用）、`revision: uint`（読取専用）、`changed()`を公開する
QAbstractListModel。どちらのプロパティもchangedをNOTIFYとする。

| role | 型 | 意味 |
| --- | --- | --- |
| `recordingKey` | string | `resume_recording`へ渡す安定キー。画面側では解釈しない |
| `programName` | string | 録画タイトルまたはファイル名 |
| `channelName` | string | 局名。不明なら空文字列 |
| `progress` | real | 0..1の視聴進捗 |
| `positionMs` | real | 保存した位置 |
| `durationMs` | real | 保存した長さ |
| `remainingMs` | real | 残り時間。0以上 |
| `sourceKind` | string | `epgstation`、`local`、`http`のいずれか |
| `recordedId` | string | EPGStation録画ID。それ以外は空文字列 |
| `videoFileId` | string | EPGStationファイルID。それ以外は空文字列 |

再生可能性は開く際に再検証する。削除されたファイル、期限切れURL、認証失敗は
既存の録画オープン失敗として通知する。資格情報をroleやエラーログへ出さない。

## 記録と再開の規則

ライブ視聴は、再生中の同一局を連続10秒視聴してから履歴へ追加する。
短時間のザッピングを除きつつ、通常の選局で履歴に残るまでの待ちを抑える値とする。
履歴はサービスIDで重複を除き、最大20件を保存する。
「前のチャンネル」は今回の実行中に実際に再生が始まった直前の別局を優先する。
10秒未満でも戻れ、連続実行で直前の2局を往復できる。起動直後は保存履歴から補う。

録画の位置は15秒間隔で保存し、停止・切替・終了時にも確定保存を要求する。
進捗95%以上、または残り30秒以内を視聴済みとし、次回は先頭から開く。
長さ不明、位置0の録画は再開一覧へ載せない。シーク中の未確定位置は保存しない。
保存履歴は最大500録画、再開一覧はそのうち最大100件とする。

## 録画の同一性と永続化

EPGStationは正規化したサーバーURLと録画ID・ファイルIDの組をキーにする。
別サーバーの同じIDや、同じ録画の異なるエンコードを混同しない。
ローカル動画は検査時にcanonicalizeした絶対パスを使い、シンボリックリンクを解決する。
HTTP動画はURLの正規化後、フラグメントを除いたURLを使う。クエリーは資源を識別し得る
ため保持する。認証情報やクエリーを含むURLは画面・ログへ表示しない。

保存先は既存設定と同じディレクトリーの`viewing-history.json`とする。
上限つきの小さなスナップショットを専用ワーカーで読み書きし、一時ファイルの同期後に
atomic renameする。設定ファイルとは独立させ、定期保存が設定の変更と競合しないようにする。
SQLiteは使用しないため、新たなSQLやsqlx検査情報は不要。
初期読込に失敗したファイルは上書きせず、セッション内だけ記録する。
通常設定を読み書きしない検証用起動では履歴もセッション内に限定する。

## QML側の接続

お気に入りボタンはserviceIdを`toggle_favorite`へ渡す。
履歴の選局には行のchannelIndexと既存の選局操作を使う。
再開一覧の決定はrecordingKeyを`resume_recording`へ渡す。

[ショートカットの仕組み](shortcut-actions-design.md)に従い、ViewerActionsに
`previousChannelAction`を用意し、Navigationスコープの`L`から実行する。
`previous_channel_available`で有効状態を決め、`previous_channel()`を呼ぶ。
既存の閉じる操作（Back / Alt+Left）とは別の操作とする。
QML製品ファイルの接続は画面担当が実装する。

## 部品テスト用API

`qml_tests`構成では`ContinueWatchingModel.load_test(json: string): bool`を公開する。
入力は上記role名をキーとするオブジェクトの配列で、実データと同じモデル通知を発行する。
`ChannelModel.load_test`は既存の`index`に加えて`favorite`（省略時false）、
`serviceId`（省略時indexの文字列）を受け付ける。これらは製品構成には含めない。

## 録画サムネイルの公開API

`RecordingModel`と`ContinueWatchingModel`に`thumbnail: string` roleを追加する。
取得済みJPEGの`data:image/jpeg;base64,...`を返し、取得前・画像なし・失敗時・
ローカル／HTTP動画では空文字列を返す。認証Cookieと再生トークンは公開しない。
`ContinueWatchingModel.load_test`ではthumbnailを省略できる（空文字列）。

EPGStation一覧の`thumbnails`の先頭IDを使い、`GET /api/thumbnails/{thumbnailId}`を
一覧と同じCookieセッションで取得する。参照元は
[固定コミットの録画型](https://github.com/stuayu/EPGStation/blob/26f56720722e61e66fc33fc0c4aacfd3d98913ba/api.d.ts)と
[サムネイル取得API](https://github.com/stuayu/EPGStation/blob/26f56720722e61e66fc33fc0c4aacfd3d98913ba/src/model/service/api/thumbnails/%7BthumbnailId%7D.ts)。
再開用履歴には画像IDだけを追加保存する。

同時取得は1件、1画像512KiB、base64化後のキャッシュは8MiB、対象は最大64画像とする。
一覧・再開モデルには現在のキャッシュ内容だけを投影する。404は画像なしとして扱い、
その他の失敗はtracingへ記録する。ログへ画像URLや認証情報を含めない。
ログインの更新時はキャッシュと未完了要求を無効化し、古い認証の結果を混在させない。
QMLでは`Image.source: thumbnail`とし、表示サイズに合わせた`sourceSize`を設定する。

## リモコン番号での選局

`ChannelModel`（channelsとrecent_channels）に`remoteKey: int` roleを追加する。
Mirakurunの`remoteControlKeyId`が1〜12ならその値、それ以外・未指定なら0。
テスト用load_testでもremoteKeyを省略できる（0）。

`Player.select_remote_key(key: int, band: string): bool`は該当局を通常の選局処理で開く。
keyは1〜12。bandは`GR`、`BS`、`CS`、`SKY`、`OTHER`を受け付ける。
`ALL`または空文字列なら現在のライブ局の放送波、なければ選択局の放送波、
いずれもなければGRを使う。未対応の放送波・番号・該当局なしではfalseを返す。
受付時trueで、接続後の失敗は既存の再生エラーへ通知する。

同じ番号の候補に現在視聴中の局が含まれる場合はその局を維持する。
それ以外はserviceIdの最小値、同値ならMirakurunのサービスIDの最小値で選ぶ。
serviceId未指定は末尾とする。主サービスを優先し、一覧の応答順や局名の変更で
選局先が変わらないようにする。番号は放送波をまたいで検索しない。
