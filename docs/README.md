# ながめTV ドキュメントポータル (Documentation Index)

ながめTV（NagameTV）の設計、仕様、開発、パッケージングに関するドキュメント一覧です。  
目的に応じて以下の各ドキュメントを参照してください。

---

## 1. ユーザー＆パッケージ導入ガイド (User & Deployment Guides)

エンドユーザー向けの導入方法や主要機能の利用ガイドです。

| ドキュメント | 概要 |
| :--- | :--- |
| **[Ubuntu debパッケージ](deb.md)** | Ubuntu 24.04 / 26.04向けdebパッケージの導入・更新・アンインストール手順 |
| **[Flatpakパッケージ](flatpak.md)** | Flatpak版の導入手順とポータル（FileTransfer等）仕様 |
| **[AppImageパッケージ](appimage.md)** | AppImage版の実行方法、glibc互換性、システム要件 |
| **[TS録画の再生](recording-playback.md)** | ローカルTSファイルの再生機能、対応形式、シーク操作、番組情報連携 |
| **[EPGStation連携](epgstation.md)** | 録画一覧・検索・TS再生、stuayu版のパスワード認証と対応範囲 |
| **[遠隔操作API](remote-control.md)** | 家庭内LANから本アプリを操作するgRPC / gRPC-Webの概要と設定方法 |
| **[APIリファレンス (Protocol Buffers)](api/remote-control.md)** | `viewer.v1.PlayerService` の詳細なRPC・メッセージ定義 |
| **[未実装機能・改善候補 (バックログ)](backlog.md)** | 実装検討中の機能や今後のロードマップ |
| **[スクリーンショット一覧](media/README.md)** | 公開用スクリーンショットと素材クレジット |

---

## 2. 開発者ガイド＆テスト環境 (Developer & Test Guides)

開発者向けの環境構築、ビルド、テスト、コーディング規約です。

| ドキュメント | 概要 |
| :--- | :--- |
| **[開発・ビルド・診断](development.md)** | 開発環境の構築（Canonical Workshop / Fedora）、ビルド、テスト、各種診断コマンド |
| **[コード規約](coding-conventions.md)** | アーキテクチャの境界、enum / Typestateの積極利用、C++ / Rust / QMLの責務分担 |
| **[GUIテスト専用環境](gui-test-environment.md)** | Weston / Xwayland / PipeWireを用いた隔離GUIテスト環境の構築と検証ルール |
| **[RustでのQt結合テスト](qt-tests.md)** | cxx-qt、gstreamer-rs、QMLを組み合わせた統合テストの設計方針 |
| **[CIとリリース](ci-release.md)** | GitHub Actionsでの継続的インテグレーションとリリースパッケージ生成 |
| **[メモリプロファイリング](memory-profiling.md)** | 長時間視聴時のメモリ増加・ヒープ割り当ての調査手法 |

---

## 3. アーキテクチャ＆機能仕様 (Architecture & Specifications)

アプリケーションの各機能における設計思想と技術仕様です。

### 画面・UI制御
| ドキュメント | 概要 |
| :--- | :--- |
| **[全体アーキテクチャ](architecture.md)** | 再生コアとUI・付随機能の境界、所有権と依存関係 |
| **[UIデザイン方針](ui-design.md)** | デザイン原則、軽快で心地よい操作感の追求 |
| **[操作への反応 (UIフィードバック)](ui-feedback.md)** | 各UIコンポーネントのアニメーションとフィードバック挙動 |
| **[ショートカットとウィンドウ操作](shortcut-actions-design.md)** | キーバインド一覧、スコープ判定、全画面（F11 / ダブルクリック）、Escの優先順位 |
| **[設定画面](settings-panel.md)** | カテゴリ別設定パネルの構成と永続化仕様 |
| **[オーバーレイ操作部の自動非表示](overlay-visibility.md)** | マウス静止時のUIフェードアウト制御 |

### 再生・メディアパイプライン
| ドキュメント | 概要 |
| :--- | :--- |
| **[共通TS入力とタイムシフト再生](ts-input-implementation.md)** | ライブ・録画共通のTS入力（tsreadex → appsrc → playbin3）とライブ振り返り機構 |
| **[音声機能と出力制御](audio-output.md)** | 音量、ミュート、複数トラック・二重音声（主/副音声）、再生時計同期、バッファ余裕設定 |
| **[チャンネル選局とブラウザー](channel-browser.md)** | 放送波分類（地デジ/BS/CS/CATV）、ソート規則、局ロゴ、カルーセルUI |
| **[番組表と予定番組 (EPG)](guide-calendar.md)** | 7日分カレンダー番組表、番組詳細情報、取得ストリーム |
| **[現在番組情報](current-program.md)** | EIT p/f解析、現在・次番組情報のリアルタイム追従 |
| **[GPU映像処理とNV12表示](gpu-video.md)** | GStreamer OpenGLプラグインとQt Quickの統合、NV12テクスチャ描画 |
| **[字幕描画](subtitle-rendering.md)** | ARIB外字フォント、常時縁取りオプション |
| **[再生速度変更](playback-speed-design.md)** | 録画再生・タイムシフト時の倍速再生制御 |
| **[デスクトップメディア連携](desktop-media.md)** | MPRIS等のOSメディアキー連携仕様 |

### 実況コメント (NX-Jikkyo)
| ドキュメント | 概要 |
| :--- | :--- |
| **[弾幕コアと表示](danmaku.md)** | 弾幕コメントの受信、レイアウト、描画パイプライン |
| **[コメント表示の仕様](comment-display-redesign.md)** | 横スクロール／ポップ（噴水）表示、文字サイズ、透過度、表示領域 |
| **[コメント投稿](comment-posting.md)** | NX-Jikkyoへのコメント投稿API連携、自分コメントの強調 |
| **[NX-Jikkyoの通信と再試行](nx-jikkyo-network.md)** | 障害時の待機延長、Retry-After、受信・勢い取得・投稿の待機共有 |
| **[実況過去ログの取得・保持](comment-archive-redesign.md)** | タイムシフト再生・録画再生時の過去ログ取得とキャッシュ |

---

## 4. 特許調査・コンプライアンス (Patent Reviews)

実況コメント機能（弾幕・一覧表示）における関連特許の調査と回避設計の記録です。

| ドキュメント | 概要 |
| :--- | :--- |
| **[特許調査と対応方針のまとめ](comment-patent-review.md)** | ドワンゴ特許等の技術照合、衝突回避の非採用、分散投入・ポップ表示による非侵害設計 |
| **[特許請求項との対比](comment-patent-claim-review.md)** | 登録公報の各請求項と現行実装の詳細対比表 |
| **[表示領域の追加調査](comment-patent-display-region-review.md)** | 映像内コメント表示領域と特許要件の幾何学的分析 |

---

## 5. 開発記録・調査アーカイブ (Historical Records & Archives)

主要機能の初期移植ログ、過去の特定問題に対する調査記録、検証履歴です。

<details>
<summary>過去のマイグレーション記録と調査ログを展開</summary>

- **機能移植・移行記録**:
  - [mainの機能移植と検証状況](feature-migration.md)
  - [実況機能の移植](comments-migration.md)
  - [言語切り替え（多言語化）の移植](localization-migration.md)
  - [再生エラーログの移植](error-log-migration.md)
  - [継続的な資源診断の移植](resource-diagnostics-migration.md)
  - [EPG変更通知の移植](epg-event-stream.md)
- **調査・技術検証メモ**:
  - [glibcとRustのメモリー制御機構の調査](allocator-controls.md)
  - [EPG有効時のRSS増加調査](allocator-investigation.md)
  - [ライブ配信の途中再開エラー調査](live-stream-errors.md)
  - [mainへの置き換えに向けた開発方針](main-replacement.md)
  - [型付きエラーとEPG取得状態の整理](refactoring-verification.md)
  - [機能分離の確認](verification.md)
  - [tsreadexによるPID変更素材の比較](tsreadex-trial.md)
  - [起動時のQt表示方式](platform-startup.md)
  - [Qt型情報変更後の増分ビルド](qml-build-order.md)
  - [Video item FFI safety](video-item-safety.md)
  - [Caller-side subtitle decoder mitigations](subtitle-decoder-mitigations.md)
  - [固定文字列による字幕メモリー計測](subtitle-memory.md)
  - [デインターレース設定と動画統計](video-statistics.md)
  - [スクリーンショット改善の作業リスト](screenshot-worklist.md)
  - [コメント表示方式の変更チェックリスト](comment-display-worklist.md)
  - [TS録画のシーク・番組情報取得ロードマップ](recording-seek-roadmap.md)
  - [録画シークとTS番組情報の実装・検証](recording-seek-verification.md)
  - [録画再生の番組情報 仕様・構成](recording-program-design.md)
  - [GitHub公開用の紹介素材作成](publicity.md)

</details>
