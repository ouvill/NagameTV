# チャンネルブラウザーとロゴ

mainの画面下部のチャンネル選択を、独立したChannelBrowserへ移植する。
Cまたは「チャンネル」ボタンで開閉し、Escapeはブラウザーを番組表より先に閉じる。
表示中は操作部の自動非表示を抑止する。カードを押すと選局してブラウザーを破棄する。
矢印キーは一覧内のカーソルを動かし、Enterで選局する。
放送種別の変更だけでは選局しない。起動時は選択中の局の種別、未選択なら先頭局の種別を使う。

## データと描画の責務

Rustのservices wire modelからhasLogoDataを取り出す。未指定はfalse。
一覧取得時だけ作るQt向けRowにlogo URLを追加する。mainのrender.rsと同じく、
ロゴがある局だけ`/api/services/{id}/logo`を指定し、それ以外は空文字列。
u64の局IDをJavaScript数値に変換しない。URLにはRustで正確な10進文字列を埋め込み、
選局要求は既存の整数indexで返す。並べ替え後のindexと絞り込み後の位置を混同しない。

ChannelBrowserは既存のrows配列を受け、種別で絞った参照配列を表示する。
横方向ListViewで表示周辺だけカードを生成し、cacheBufferは0。
閉じるとLoaderが一覧とカードを破棄する。QtのImageは非同期取得、sourceSizeは128×72、
グローバル画像キャッシュは無効。繰り返し開く・スクロールする際の再取得と引き換えに、
訪問した全局の画像をアプリ側の長期キャッシュに保持しない。
ロゴ未取得・欠損・失敗時にはTVの表示を使い、選局機能は維持する。
sourceSizeは表示用画像のサイズ指定であり、HTTP受信サイズの上限ではない。

参照したQt API:

- [ListView](https://doc.qt.io/qt-6/qml-qtquick-listview.html)：必要時のdelegate生成、currentIndexとキーボード操作。
- [Image](https://doc.qt.io/qt-6/qml-qtquick-image.html)：asynchronous、sourceSize、cache、status。

## 検証と残作業

Rustの投影テストでロゴなし・ロゴあり・u64最大値の正確なURL・空一覧を検証。
QMLで種別変更時の無選局、Enterで元indexの選局、空種別、一覧置換、
500局で生成カード20個未満を検証した。Cの操作と文字入力へのC配送も検証。
Rust48件成功、外部TSが必要な1件は未実行。QMLスイート29件成功。
Clippy全ターゲット警告なし、CMakeビルド成功。

実サーバーでNHKロゴとブラウザーの表示を画像確認し、カードから
3209641984→3203246080へ選局、切替後のPLAYING通知と正常終了を確認した。
証跡は`benchmark/channel-browser/`のsmoke.py、smoke.log、画像。
実アプリのCキー、実通信時の繰り返し開閉・高速スクロール・長時間の資源測定は未検証。

mainのカードにある各局の現在番組・放送時間・進行率・実況勢い、EPG連動サブ局整理、
外側クリックで閉じる操作と詳細な画面デザインは残作業。ブラウザー全体の機能互換は未完了。
