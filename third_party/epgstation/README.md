# EPGStation commentary mapping

`rust/src/features/comments/mapping.rs` adapts the channel tables from
[stuayu/EPGStation JikkyoUtil.ts](https://github.com/stuayu/EPGStation/blob/2aad00be11139d6484f7dcec61ad34c0cdd1cb9e/client/src/util/JikkyoUtil.ts).

Snapshot: `2aad00be11139d6484f7dcec61ad34c0cdd1cb9e`. The upstream tables were generated from KonomiTV's
`jikkyo_channels.json`, originally based on NicoJK's `jkch.sh.txt`.
The upstream MIT license is preserved in [LICENSE](LICENSE).

To update, compare both upstream tables and port their numeric entries; retain
the attribution and run the comments tests. No mapping is downloaded at runtime.
