# viewer-mpegts

MPEG-2 Transport Stream の 188 バイトパケット、section 再構成、CRC-32、PAT/PMT の構文を扱う、Qt に依存しないクレートです。PAT の複数 section は `Pat` で集約できます。PSI、private section、SDT/EIT/TDT/TOT 用の section 再構成は `Sections` と `SectionKind` で選びます。

ARIB の字幕・データ放送記述子の意味付けは利用側が行います。アプリの `transport` は字幕 ES を選択し、`arib-b24` はデータ放送用の DSM-CC PID とカルーセルを扱います。

リポジトリーのルートから `python3 scripts/test.py viewer-mpegts` で機器不要のテストを実行できます。
