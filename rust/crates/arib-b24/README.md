# ARIB STD-B24 データカルーセルデコーダー

[STD-B24 全体の実装対応リスト](../../../docs/std-b24-implementation.md)に、対応範囲と残件をまとめています。

STD-B24 第三編第6章の DSM-CC データカルーセルを扱う、Qt に依存しないクレートです。
[ARIB 公開の英訳（第5.1版）](https://www.arib.or.jp/english/html/overview/doc/6-STD-B24v5_1-3p3-E2.pdf)の
DII、DDB、DSM-CC section の構造を実装しています。
TS パケット、section 再構成、PAT/PMT の汎用構文は `viewer-mpegts` クレートを利用します。

`SectionPackets` は 188 バイト TS パケットの連続性を確認し、PID ごとに section を
組み立てます。`pmt_pid_from_pat` と `data_components_from_pmt` で選択中の
サービスの PMT と BML 用 DSM-CC PID を取得できます。`TsReceiver` はデータ PID の
DII/DDB section を取り出します。
`Section::parse` は *完全な* section の長さ、ヘッダーの識別子、MPEG-2 CRC-32、
DII/DDB のフィールドを検査します。
`Carousel::new` に DII を渡し、同じカルーセルの DDB を `push` すると、
モジュールの全ブロックがそろった時点で `Module` を返します。重複ブロック、
古いモジュール版、順不同の到着を扱います。モジュールに CRC 記述子がある場合は
完成時に照合します。`DataReceiver` はこれらを接続し、同じ版の複数 DII を統合します。
`Module::decode` は圧縮種別 0 の zlib モジュールを元サイズを検査して展開します。
圧縮ストリームの終端・チェックサムを検証します。末尾に CRC32・ISIZE の8バイトが
追加される送出形式も、両方が展開結果と一致する場合に受け入れます。
名前・MIME タイプを含む記述子は生のバイト列として保持します。
`EventSection::parse` は table `0x3D` のイベントメッセージ、時刻指定、
識別子と固有データを解析します。`SectionPackets` で組み立てた section を渡せます。

`ServiceReceiver::new(service_id)` に連続した TS パケットを渡すと、PAT/PMT を追跡し、
選択サービスのデータ PID から `Resource` を返します。`push_items` はリソースに加えて
型付きのイベント section も返します。外部のチューナー、録画ファイル、ネットワーク入力から
188 バイトの TS パケットを供給してください。`DecodedModule::resources` は、直接対応する
1 モジュール 1 素材と、HTTP エンティティ形式の複数素材を個別の名前・メディア種別・本文に
分けます。名前は放送の文字コードを保つバイト列です。`Resource::save_to` は呼び出し側が
指定した新規ファイルに本文を書き出し、放送由来の名前を勝手にパスへ変換しません。
HTTP ヘッダーの順序と行の折り返しを扱い、`ResourceMapping` で直接対応とエンティティを
区別します。素材の件数や名前から対応形式を推測する必要はありません。
`ServiceReceiver::program` は検証済み PAT/PMT による TS ID・サービス ID・PCR PID を返します。
SDT/EIT/TOT の番組情報・放送時刻の解析は、このクレートの外側で行います。

```rust,no_run
use arib_b24::transport::{BroadcastItem, ServiceReceiver};

# fn receive(packet_stream: impl Iterator<Item = [u8; 188]>) -> Result<(), Box<dyn std::error::Error>> {
let mut receiver = ServiceReceiver::new(42)?;
for packet in packet_stream {
    for item in receiver.push_items(&packet)? {
        match item {
            BroadcastItem::Resource(resource) => {
                // resource.name / media_type / data を一覧表示や保存に利用する。
                // 保存先は利用者が選び、resource.save_to(path) に渡す。
                println!("module={} bytes={}", resource.module_id, resource.data.len());
            }
            BroadcastItem::Event(event) => {
                println!("data event={}", event.data_event_id);
            }
        }
    }
}
# Ok(())
# }
```

イベントの時刻に従った実行、BML・スクリプト・画像の解釈、画面表示は
まだ実装していません。DII でサイズ不明のモジュールは再構成を拒否します。
メモリ上限の既定値はモジュール・展開後のそれぞれ 256 MiB、連結後のファイルは 1 GiB です。
`DecodeLimits::new`、`with_linked_bytes`、`ServiceReceiver::with_limits` で受信側のメモリ量に合わせて変更できます。
規格の1モジュール最大256 Mbytesは、DDBの16ビットブロック番号に由来します。
ModuleLink descriptor で分割されたファイルは、先頭から末尾までのモジュールがそろうと
指定された順序で連結し、先頭モジュールの名前・メディア種別を持つ1つの素材として返します。
断片が届く順序は問いません。単独の断片に `DecodedModule::resources` を呼ぶとエラーになります。

機器不要の検証はリポジトリーのルートから `python3 scripts/test.py arib-b24` で実行します。
