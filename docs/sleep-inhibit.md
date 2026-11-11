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

経路は `portal → logind + ScreenSaver → logind + ネイティブX11` の順に選ぶ。
画面消灯とサスペンドの両方を取得した経路だけを `inhibited` とする。

| 手段 | 採用範囲・所有する資源 | 解除 |
| --- | --- | --- |
| `org.freedesktop.portal.Inhibit` | 第一選択。IdleとSuspendの両フラグを要求する | `Request.Close` と専用接続の破棄 |
| `org.freedesktop.ScreenSaver` | portal利用不可時の画面側の代替。logindの取得も必須 | `UnInhibit(cookie)` と専用接続の破棄 |
| `org.freedesktop.login1.Manager.Inhibit` | 代替経路のサスペンド側。`sleep:idle` / `block` のFDを所有する | FDを閉じる |
| XScreenSaverSuspend | ScreenSaverもないネイティブX11で画面側を補う。logindの取得も必須 | Suspend解除と専用X接続の破棄 |
| DPMS全体設定の変更 | 採用しない。他の利用者の設定・所有権と競合するため | — |

代替ではlogindを先に取得し、続いて画面側を取得する。片側が失敗した場合は、
取得済みの資源も解放する。ScreenSaverだけ、またはlogindだけの成功を両方の成功として扱わない。
選んだ経路は取得時にINFO、利用不可による経路切替は原因とともにWARNへ記録する。

`ServiceUnknown`、必要なインターフェース・オブジェクトの不在、バス不在等は、
環境の事実としてWARNを一度記録して保持する。停止・一時停止・終端からの再生で
同じ利用不可を調べ直さない。設定OFF/ONでキャッシュを捨て、またはportal・ScreenSaver・
logindの `NameOwnerChanged` で利用不可を再検査できる状態へ戻す。
`unavailable` は設定ONのまま停止しても保持する。利用不可が解消され、再生していなければ `idle` となる。

タイムアウト・拒否・不正応答はERRORに記録し、その視聴中には繰り返さない。
次の視聴試行では再試行する。portalで拒否された要求を代替で回避しない。
動作中の代替経路はportal出現だけでは取り直さず、次の視聴から第一選択へ戻す。

### portal

[Inhibitの公式仕様](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Inhibit.html)と
[Requestの公式仕様](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Request.html)に従う。
要求ごとの専用D-Bus接続で非同期呼出しを行い、最大1.5秒で打ち切る。
`Request.Response(0)` とメソッドのハンドル返却の両方を確認してから取得済みとする。
通知は要求前に購読する。Responseがない環境も成功とは扱わない。

通常解除のリモート応答は待たず、接続消失によるportal側の後始末も解放の保証とする。
未完了要求の取消し・終了時の切断は正常な取消しであり、遅れて成功した要求も
接続の寿命を超えて保持しない。ローカル送信失敗は原因とともに記録する。
この判断は[上流実装](https://github.com/flatpak/xdg-desktop-portal/blob/main/desktop-portal/inhibit.c)の
非同期完了処理に基づく。

### ScreenSaverとlogind

[ScreenSaver仕様](https://specifications.freedesktop.org/idle-inhibit/0.1/index.html)の
標準パス `/org/freedesktop/ScreenSaver` と `Inhibit` / `UnInhibit` を使う。
このAPIの影響範囲はセッション実装によって異なるため、サスペンドは
[logind仕様](https://www.freedesktop.org/software/systemd/man/latest/org.freedesktop.login1.html)の
FDロックで補う。どちらも専用D-Bus接続と非同期呼出しを使う。

logindはsystem busに接続する。必要な権限がなければ失敗として公開し、権限昇格や
認証ダイアログは要求しない。Flatpakでは引き続きportalを第一選択とし、代替のための
追加のsandbox権限は要求しない。OSの管理者操作等、抑止を無視する権限を持つ操作は防げない。

### ネイティブX11

Qtが実際に使っているローカルxcb表示先を検出し、専用X接続とScreenSaver拡張1.1を
検証してから `XScreenSaverSuspend` を使う。X11/Xssライブラリーは実行時に解決し、
存在しなければ利用不可にする。DPMSの有効設定・タイムアウトは変更しない。

[このAPI](https://xorg.freedesktop.org/archive/X11R7.5/doc/man/man3/XScreenSaverUnregister.3.html)は
XサーバーのscreensaverとDPMSタイマーを一時停止する。WaylandやXwaylandの外側の
コンポジターには効かないため、QtのWayland表示、XWAYLAND拡張、Waylandセッションの
表示先を検出した場合は製品経路として採用しない。

## 検証と限界

`python3 scripts/test.py app` で偽バックエンドによる経路・所有・猶予期限・利用不可の保持と
再検査を確認する。`python3 scripts/test.py sleep-inhibit` は私的D-Busの偽portal・
ScreenSaver・logindで、取得、cookie/FDの解放、片側失敗、拒否、所有者変更を確認する。
QCoreApplicationだけを使い、画面・GPU・音声を必要としない。
試験のsession busとsystem busはどちらも私的バスへ向け、サービスの自動起動も無効にする。

`python3 scripts/test.py sleep-inhibit-x11` は公開GUIランナーの専用Xwaylandで、
製品経路の拒否と、明示的なwire確認用経路の取得・解除を検証する。
専用表示・実GPU・仮想音声の検証に失敗したら停止し、ソフトウェア描画へ切り替えない。
wire確認用経路はQMLへ公開せず、製品の経路選択にも使わない。

実ネイティブXorg、物理画面、自動サスペンド、Flatpakの実環境での実効性は別途確認する。
portalは許可後の各フラグの実効性を返さないため、バックエンドが成功と答えた場合でも
OS側の実装や方針による制約がある。サービス所有者の消失とバス切断は失敗として検出する。
