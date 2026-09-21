# チャンネル分類と選局の移植（過去の検証記録）

> [!NOTE]
> 本ドキュメントの内容は **[チャンネル選局とブラウザー](channel-browser.md)** に統合されました。  
> 放送波の分類規則、並び順ルール、局ロゴ、チャンネルブラウザーUIの最新仕様は統合先のドキュメントを参照してください。

以下は、チャンネル分類および選局ロジックの実装・移植時の記録です。

---

## 過去の実装・検証履歴 (2026-09-07)

mainの `viewer-core/src/channels.rs` とMirakurunのService実装を参照し、サービス一覧を解釈する処理を `channels` モジュールへ分離した。

### 順序と選択
- 放送種別は `Band` enum（GR、BS、CS、SKY、Other）。
- 地デジは `remoteControlKeyId`、その他は `serviceId` で並べる。
- 同一番号は表示ラベル順、次に `serviceId` の順。
- 保存した局は一覧位置ではなくu64のサービスIDで復元。

### 検証
Rustテストで種別優先順、欠落／0のリモコン番号、同番号の順序、重複・非TV除外、不正JSON、u64保持を確認。Qt Quick TestでComboBoxでの絞り込みと元indexの選局通知を検証。
