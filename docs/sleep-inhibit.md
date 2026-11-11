# 視聴中のスリープ・画面消灯の抑止

「視聴中は画面を消さない」は既定ONとする。Linuxで実際に再生している間、
画面のアイドル消灯と自動サスペンドを抑止する。ホーム画面へ戻っても再生が続く間は対象となる。
一時停止、停止、読込失敗、録画終端、終了では解除する。

## QML公開API

以下を `Player` に公開する。この契約は設定UIとの合意として固定する。
すべてのプロパティは読み取り専用で、通知は共通の `sleep_inhibit_changed()` を使う。
通知の前に設定と現在状態をまとめて確定する。

| 種類 | 名前・型 | 意味 |
| --- | --- | --- |
| property | `sleep_inhibit_enabled: bool` | 保存対象の設定値。既定 `true` |
| property | `sleep_inhibit_state: QString` | 下表の固定文字列。表示ラベルはQMLで翻訳する |
| property | `sleep_inhibit_error: QString` | 翻訳済みの利用者向けエラー。正常時は空文字列 |
| invokable | `configure_sleep_inhibit(enabled: bool) -> void` | 設定を変更し、即時に取得・解除を開始して既存の保存処理を呼ぶ |
| signal | `sleep_inhibit_changed()` | 設定・状態・エラーのいずれかが変化したとき。言語変更でも通知する |

| `sleep_inhibit_state` | 意味 |
| --- | --- |
| `disabled` | 設定がOFF |
| `idle` | 設定はONだが再生していない |
| `acquiring` | 抑止の取得中。取得成功を示さない |
| `inhibited` | portalの成功通知を確認し、抑止ハンドルを所有している |
| `unavailable` | セッションバス、portal、必要なインターフェース、対応OSが利用できない |
| `failed` | 抑止要求または取得後の接続維持に失敗した |

UIは設定値と実際の状態を区別する。`inhibited` 以外を「抑止中」と表示しない。
エラー文に操作方法やD-Busの生の診断文字列を混ぜず、詳細な原因はstderrへ記録する。
`--features` の明示指定時は他の設定と同様に一時設定となり、保存ファイルを書き換えない。

## 判断と所有

取得・解除の判断と状態はQt非依存のRust型に置く。取得操作と取得済みハンドルを
排他的なenumのvariantで所有し、解除時は所有権を消費する。QML部品の寿命には依存しない。

録画HTTPが再接続中のときと再生を継続するシークでは、既存ハンドルだけを最大5秒保持する。
この猶予中に新たな取得は行わない。再開すれば同じハンドルを使い、5秒を超えれば解除する。
初回読込では取得せず、一時停止・明示停止・失敗・終端では猶予を使わない。
再接続が続く間に猶予を延長しない。ライブ配信の復帰拒否からの再試行は旧セッションの
失敗として解除し、新セッションの再生確認後に取得する。接続の各試行では取得しない。

## Linuxとの接続

`org.freedesktop.portal.Inhibit` に Idle と Suspend を要求し、解除には返却された
`org.freedesktop.portal.Request.Close` を使う。Flatpak内外で同じ経路を使う。
[Inhibitの公式仕様](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Inhibit.html)と
[Requestの公式仕様](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Request.html)に従う。

ScreenSaver等への自動フォールバックは行わない。画面消灯だけの抑止をサスペンドまで
成功したように扱うことを避け、バス・portal・要求の失敗を状態とログへ反映するためである。
要求ごとに専用のD-Bus接続を所有し、`Inhibit` は非同期で呼び出す。メソッド応答は
最大1.5秒で打ち切る。`Request.Response(0)` とメソッドのハンドル返却の両方を
確認してから取得済みとする。通知は要求前に購読し、通知が先着する場合も保持する。
取得後もportalの所有者変更と接続断を検出し、失敗へ遷移する。
取得失敗はその視聴中にポーリングで繰り返さず、停止・一時停止または設定OFF/ONで再試行できる。

通常解除では `Request.Close` を送信して専用接続を破棄する。解除完了の保証は呼出元の
接続消失によるportal側の後始末に置くため、Closeのリモート応答は待たない。
ローカル送信失敗は操作名・原因付きで記録する。未完了要求の取消しと終了時の切断は
正常な取消しとして扱い、遅れて成功した要求も接続の寿命を超えて保持しない。
成功通知が示すのはportalバックエンドの応答成功であり、物理画面やOS設定の実効性は
デスクトップ側の実装に依存する。

## 検証

`python3 scripts/test.py app` で偽inhibitorによる所有・解除・猶予期限と設定保存を検証する。
`python3 scripts/test.py sleep-inhibit` は私的な `dbus-run-session` と偽portalで、
取得、解除、取得中の取消し、portal不在、拒否、不正応答、取得後の所有者消失を検証する。
QCoreApplicationのみを使い、画面・GPU・音声・ホストのセッションバスを必要としない。
私的バスにはサービス自動起動用のディレクトリーを設定せず、実portalの自動起動も防ぐ。
実portalと物理的な消灯・サスペンドの確認はこの試験の対象外である。

古いportal等が `Request.Response` を返さない場合は、期限切れを失敗として表示する。
メソッドの成功だけではバックエンド失敗を見落とすためである。
この判断は[上流のInhibit実装](https://github.com/flatpak/xdg-desktop-portal/blob/main/desktop-portal/inhibit.c)の
非同期完了処理に基づく。

portalは許可後の各フラグの実効性を返さないため、IdleとSuspendの両方がOS側で
適用されたことをアプリ単独では個別に確認できない。実環境では両方を確認する。
