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
| `inhibited` | portalへの取得要求が成功し、抑止ハンドルを所有している |
| `unavailable` | セッションバス、portal、必要なインターフェース、対応OSが利用できない |
| `failed` | 抑止要求または取得後の接続維持に失敗した |

UIは設定値と実際の状態を区別する。`inhibited` 以外を「抑止中」と表示しない。
エラー文に操作方法やD-Busの生の診断文字列を混ぜず、詳細な原因はstderrへ記録する。
`--features` の明示指定時は他の設定と同様に一時設定となり、保存ファイルを書き換えない。

## 判断と所有

取得・解除の判断と状態はQt非依存のRust型に置く。取得操作と取得済みハンドルを
排他的なenumのvariantで所有し、解除時は所有権を消費する。QML部品の寿命には依存しない。

再生中からの短い再接続と再生を継続するシークでは、既存ハンドルだけを最大5秒保持する。
この猶予中に新たな取得は行わない。再開すれば同じハンドルを使い、5秒を超えれば解除する。
初回読込では取得せず、一時停止・明示停止・失敗・終端では猶予を使わない。
再接続が続く間に猶予を延長しない。

## Linuxとの接続

`org.freedesktop.portal.Inhibit` に Idle と Suspend を要求し、解除には返却された
`org.freedesktop.portal.Request.Close` を使う。Flatpak内外で同じ経路を使う。
[Inhibitの公式仕様](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Inhibit.html)と
[Requestの公式仕様](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Request.html)に従う。

ScreenSaver等への自動フォールバックは行わない。画面消灯だけの抑止をサスペンドまで
成功したように扱うことを避け、バス・portal・要求の失敗を状態とログへ反映するためである。
実装と検証の詳細は後続の変更で追記する。
