# 弾幕のコアと表示

## 責務の分担

弾幕のコアは `rust/crates/viewer-comments/src/danmaku.rs` に置く。
Qt、GPU、ネットワーク、実時間の時計に依存しない。テストから経過時間を注入できる。

| 層 | 所有する状態と責務 |
| --- | --- |
| Rust Engine | 型付きコメント、録画タイムライン、再生カーソル、行の占有、衝突判定、表示時間、一時停止、シーク、寿命 |
| Rust DanmakuController | Qtとの型付き境界、単調増加時計、計測要求、生成・削除通知、件数プロパティ |
| QML DanmakuOverlay | フォントと文字幅の計測、Label、NumberAnimation、透明度、UIオブジェクトの生成・破棄 |
| QML DanmakuTimeline | 再生位置・再生状態をRustへ渡すバインディングとメソッド転送のみ |

QML に行選択、衝突判定、ソート、再生カーソル、寿命の判定は置かない。
QMLの `visuals` は数値IDと表示オブジェクトの対応だけを保持する。コメントのコアモデルではない。
`activeCount` はQML配列の長さではなくRustの件数を参照する。

```mermaid
sequenceDiagram
    participant R as Rust Engine / Controller
    participant Q as QML 表示
    R->>Q: 計測要求（ID・本文）
    Q->>R: 計測結果（ID・幅）
    Note over R: 行・衝突・表示時間・寿命を決定
    R->>Q: 生成（ID・種類・本文・色・幅・行内座標・時間）
    Note over Q: Qt Quickがアニメーション
    Q->>R: 描画フレームの通知
    Note over R: 単調増加時計で期限を判定
    R->>Q: 期限切れIDの削除
```

毎フレーム全ラベルの座標をRustからQMLへ転送しない。Qt Quickは表示の補間を行い、
Rustは同じ軌道式と生成時の速度から衝突と期限を判定する。
縦方向も3種類の行グループの開始位置だけを通知し、ラベルごとの座標更新を送らない。
QMLのアニメーション完了通知はコア状態を変更しない。
FrameAnimationは表示中コメントが存在して一時停止していない時だけRustのtickを呼ぶ。
受信・計測結果・設定変更時にも時計を進めるため、描画フレームが遅れても古い占有を使わない。

## 型と資源

位置はRustの `Position` enum、色は検証済みの `Color`、時刻は `Duration`、表示IDは `Id`。
ライブ入力とJSONインポートの境界で本文、色、種類、時刻を検証する。QMLとの通知には
QString・整数・実数の型付き引数を使い、描画状態をJSONで毎フレーム送り直さない。

計測待ちは最大1件。クリア前の計測結果はIDで拒否する。IDは再利用せず、u32を使い切ると
新しい要求を拒否するため、JavaScriptの整数精度や古い応答との衝突に依存しない。

衝突判定は候補行の表示中コメントだけを走査する。行ごとの格納領域は使用時に確保する。
空き行がなければLabelを作らない。固定64件の制限は設けず、画面と文字サイズと空き行が
表示数を決める。寿命更新は表示中要素を走査し、期限切れIDだけを返す。
全表示のスナップショットや新しい本文配列をフレームごとに生成しない。
clearで占有領域と計測待ちを解放し、resetでは録画タイムラインの保持領域も解放する。
Rustの借用はQtシグナル発行前に終了するため、QMLから同期で計測結果が返っても再入可能。

## ライブ表示と互換範囲

NX-JikkyoのmailをRustで解析し、位置 `naka` / `ue` / `shita` と標準色・末尾2の色・
`#RRGGBB` を保持する。履歴は一覧のみ、新着を描画へ渡す。
本文はPlainText。改行はRustで空白に変換し、1件で1行を使う。

8行固定を廃止し、使用可能な高さと `max(30, フォント高さ + 6)` からRustで行数を決める。
番組名と上下余白を避ける。横流れは10pxの間隔と追いつき判定、固定は空き行を使う。
同じ行が混雑したコメントは見送り、後から遅延表示しない。異なる表示形式の行は独立するため、
横流れと固定など種類をまたぐ重なりはあり得る。重ねて全件を出すunlimitedモードは未提供。

横流れは右端からすぐ入り、通常5秒／全画面8秒。上下固定は通常4秒／全画面6秒。
設定の速度倍率で割り、既存コメントは生成時の速度を保つ。
画面寸法、文字サイズ、全画面状態の変更は既存配置をクリアする。
Player UIの表示時は上の番組名と下の操作パネルを避ける。Rustが占有中の行数と上下の余裕を
計算し、横流れ／上固定の開始位置を下へ、下固定を上へ動かす。QMLでは種類ごとの親Itemを
180msでアニメーションさせ、既存と新着が同じ移動を共有する。横移動・表示期限・行間隔は保つ。
全行使用中など上下の領域へ収まらない場合は、画面外へ押し出したり文字を潰したりせず、
移動量を制限する。既存コメントはUIに重なる場合があるが、消去はしない。
新着は上下のUIを避ける行だけに限定し、配置できなければ見送る。期限切れで余裕ができると
移動量を再計算する。UIを隠すと開始位置も戻る。
移動途中に新着が届く場合は、移動元と移動先の両方で画面内に収まる行だけを使う。
連続開閉でアニメーションが反転しても、この範囲を保持して下端・上端のはみ出しを防ぐ。
同じ種類の行間隔は一定。種類をまたぐ重なりの扱いは従来どおり。
一時停止はRust時計とQtアニメーションの両方を止める。
非表示状態もRustへ渡し、録画時計が進んでも計測待ちやLabelを生成しない。非表示中に過ぎたコメントは再表示時に出さない。
現在のアプリの停止ボタンはLoaderごと破棄する操作であり、一時停止とは別。

受信キュー256件、poll上限64件、履歴200件は描画上限とは別のため維持する。
投稿、文字サイズ／フォントコマンドは含まない。

## 録画側から使う契約

録画ファイルの選択・再生、過去ログ取得、放送時刻から動画内時刻への変換は未実装。
将来の再生オブジェクトから以下のように接続する。

```qml
DanmakuTimeline {
    id: timeline
    overlay: danmakuOverlay
    position: recording.positionSeconds
    playing: recording.playing && !recording.buffering
}
```

recordingは将来の接続先の例で、現在のPlayerのAPIではない。
QMLからインポートする場合は `timeline.load(jsonString)` を使う。
型検証・データ保持・ソートはRustで行う。Rustからは `Engine::load(Vec<TimedComment>)` を使える。

```json
[
  {"time":1.25,"text":"横流れ","type":"right","color":16777215},
  {"time":3,"text":"上固定","type":"top","color":"#ff0000"},
  {"time":3,"text":"下固定","type":2}
]
```

- timeは動画先頭からの秒数（小数可）、typeはright/top/bottomまたは0/1/2。
- 不正入力はfalseとcontroller.errorを返し、既存データを維持する。
- 時刻を過ぎたコメントを一度ずつ投入する。混雑で表示しなかったものも消費済みとする。
- 前後の明示シークで `timeline.seek(targetSeconds)` を呼び、動画時計を更新する。
  Rustは二分探索でカーソルを設定し、表示を消す。QMLのpositionバインディングは変更しない。
- 時計が後退した場合もRustが自動リセットする。前方シークは通常の時計進行と区別するため
  明示呼び出しが必要。途中まで流れていたコメントの位置は復元しない。
- playing=falseで投入と移動を止め、clear()でデータと表示を解放する。
- 表示開始は動画時計、表示後の移動はアニメーション時計。動画倍速への移動速度の自動追随や
  遅延分の途中位置補正は含まない。大規模な過去ログの分割取得は取得側で今後設計する。

## 検証

CPUテストで、可変行数・64件超・混雑時の見送り・速度変更時の衝突判定、表示時間、
一時停止と期限切れ、古い計測応答の拒否、解放、録画時刻順・シーク・入力検証を確認する。
QMLテストは実際のRust型を静的リンクしたQt Quick Testランナーを使い、生成通知から
Label表示、時間経過によるRustの削除通知、停止／再開、録画カーソルとの連携を確認する。
モックのスケジューラーは使わない。

```sh
CARGO_TARGET_DIR=build/comments-protocol cargo test --locked --manifest-path rust/crates/viewer-comments/Cargo.toml --features network
scripts/test-danmaku.sh
cmake --build build
```

scripts/test-danmaku.shはX11とGPUを検出・検証し、不足時には停止する。
QtQuickTestは開発用qml_tests featureのみでリンクし、通常ビルドへは追加しない。
QML内にRust型があるため、弾幕を含む試験では単体qmltestrunnerでなくこのランナーを使う。
長時間RSS、CPU/GPUのフレーム時間、録画映像との実同期、実サービスの色・位置指定は未測定。

## 参照

挙動の比較元は [DPlayer danmaku.js (bc43cc0)](https://github.com/DIYgod/DPlayer/blob/bc43cc0ac2470f3b3552414a81e64b913b107bb3/src/js/danmaku.js)。
受信形式は [NX-Jikkyo](https://github.com/tsukumijima/NX-Jikkyo)、
UI補間は [Qt Animation](https://doc.qt.io/qt-6/qml-qtquick-animation.html) を参照。

2026-09-08の修正後の確認: Rust crate 30件成功・外部サービス用1件除外、
実Rust型を使うQML試験116件成功（初期化・終了を含む）。
DISPLAY=:0のX11とNVIDIA RTX 4070 TiのOpenGLを検証してから実行した。
QML静的検査は生成済みの型情報と実QMLファイルを配置したimport pathで警告なし。
Qtテストはプロセスのメインスレッドで実行し、通常版にテスト用CLI分岐とQtQuickTestを含めない。
通常のCMakeリリースビルドと、Rust全ターゲットのClippy（警告をエラー扱い、開発用featureを含む）も成功。
通常バイナリーの依存ライブラリーにQtQuickTest/QtTestが含まれないことを確認した。
