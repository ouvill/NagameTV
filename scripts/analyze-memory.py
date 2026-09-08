#!/usr/bin/env python3
"""Archive one process's diagnostics and build an offline HTML memory report."""
import argparse
from datetime import datetime, timezone
import html
import json
import os
from pathlib import Path
import shutil
import sys


def read_samples(paths):
    samples = {}
    gc_count = 0
    invalid = 0
    for path in paths:
        with path.open(encoding="utf-8") as source:
            for line in source:
                try:
                    value = json.loads(line)
                    if value.get("kind") == "qt_gc":
                        gc_count += 1
                        continue
                    record = value["record"]
                    key = (int(record["unix_ms"]), int(record["elapsed_ms"]))
                    samples[key] = value  # Detail/history overlap is intentional.
                except (ValueError, KeyError, TypeError, AttributeError):
                    invalid += 1
    return sorted(samples.values(), key=lambda v: v["record"]["unix_ms"]), gc_count, invalid


def metric(row, name):
    group, field = name.split(".")
    data = row.get("record", {}).get("snapshot", {}) if group == "snapshot" else row.get(group)
    value = (data or {}).get(field)
    return value if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def chart(rows, title, series):
    # Bucket min/max preserves brief peaks without enormous SVGs for long sessions.
    first, last = rows[0]["record"]["unix_ms"], rows[-1]["record"]["unix_ms"]
    values = [metric(r, key) / divisor for r in rows for key, _, divisor in series
              if metric(r, key) is not None]
    ceiling = max(values, default=1) or 1
    colors = ["#1565c0", "#d84315", "#388e3c", "#7b1fa2"]
    parts = [f'<h2>{html.escape(title)}</h2><svg viewBox="0 0 1000 270" role="img" aria-label="{html.escape(title)}">',
             '<path d="M65 15V235H980" fill="none" stroke="#999"/>']
    for i in range(5):
        y = 235 - i * 55
        parts.append(f'<text x="5" y="{y}" font-size="12">{ceiling*i/4:.1f}</text>')
    for index, (key, label, divisor) in enumerate(series):
        buckets = {}
        for row in rows:
            value = metric(row, key)
            if value is None:
                continue
            x = 65 + 915 * (row["record"]["unix_ms"] - first) / max(1, last-first)
            buckets.setdefault(int(x), []).append((x, 235 - 220 * value / divisor / ceiling))
        points = []
        for bucket in buckets.values():
            chosen = {0, len(bucket)-1, min(range(len(bucket)), key=lambda i: bucket[i][1]),
                      max(range(len(bucket)), key=lambda i: bucket[i][1])}
            points.extend(bucket[i] for i in sorted(chosen))
        coords = " ".join(f"{x:.1f},{y:.1f}" for x, y in points)
        parts.append(f'<polyline points="{coords}" fill="none" stroke="{colors[index]}" stroke-width="1.5"/>')
        parts.append(f'<text x="{65+index*220}" y="265" fill="{colors[index]}" font-size="12">{html.escape(label)}</text>')
    parts.append('</svg><p>横軸：開始から %.1f 時間。欠測区間は線で接続されます。</p>' % ((last-first)/3600000))
    return "".join(parts)


def time_label(row):
    return datetime.fromtimestamp(row["record"]["unix_ms"]/1000, timezone.utc).isoformat(timespec="seconds")


def delta(a, b, key, divisor=1):
    before, after = metric(a, key), metric(b, key)
    return None if before is None or after is None else (after-before)/divisor


def number(value):
    return "欠測" if value is None else f"{value:+.2f}"


def report(rows, gc_count, invalid):
    rss = [metric(r, "process.rss_kib") for r in rows]
    rss = [r/1024 for r in rss if r is not None]
    gaps = [(b["record"]["unix_ms"]-a["record"]["unix_ms"])/1000 for a,b in zip(rows,rows[1:])]
    dropped = max((r.get("dropped_records", 0) for r in rows), default=0)
    content = ['<!doctype html><meta charset="utf-8"><title>メモリ診断</title>',
               '<style>body{font:15px system-ui;margin:32px auto;max-width:1100px;padding:0 20px;color:#223}svg{width:100%;background:#f8fafc}table{border-collapse:collapse;width:100%}td,th{padding:8px;border-bottom:1px solid #ddd;text-align:left}code{overflow-wrap:anywhere}h2{margin-top:32px}</style>',
               '<h1>普段の視聴のメモリ診断</h1>',
               f'<p>{time_label(rows[0])} 〜 {time_label(rows[-1])}（UTC） / {len(rows)} 計測点</p>',
               f'<p>RSS最大：{max(rss):.1f} MiB</p>' if rss else '<p>RSS：欠測</p>',
               f'<p>最大計測間隔：{max(gaps, default=0):.1f}秒 / 診断破棄カウンター最大：{dropped} / GCログ行：{gc_count} / 読めなかった行：{invalid}</p>',
               '<p>RSSはプロセスの常駐量です。glibc使用中とmalloc mmapは確保量であり、RSSと直接差し引いてQtやGPUの使用量にはできません。増加区間の操作は相関であり、原因やリークの確定ではありません。GC行数はGC回数ではありません。</p>']
    content.append(chart(rows, "メモリ推移（MiB）", [("process.rss_kib", "RSS", 1024), ("process.pss_kib", "PSS", 1024), ("process.anonymous_kib", "匿名メモリ", 1024)]))
    content.append(chart(rows, "glibc確保量（MiB）", [("allocator.in_use_bytes", "arena使用中", 1048576), ("allocator.free_bytes", "arena空き", 1048576), ("allocator.mmap_bytes", "malloc mmap", 1048576)]))
    content.append(chart(rows, "保持件数", [("snapshot.subtitle_cells", "字幕セル", 1), ("snapshot.subtitle_pending", "字幕待機", 1), ("snapshot.comment_history_count", "コメント履歴", 1)]))
    content.append(chart(rows, "EPG保持量（MiB）", [("snapshot.epg_text_capacity_bytes", "EPG文字列容量", 1048576)]))
    content.append(chart(rows, "OSリソース件数", [("process.threads", "スレッド", 1), ("process.fds", "FD", 1)]))
    content.append('<h2>RSS増加の大きい計測区間（最大20件）</h2><p>区間末尾のイベント・状態を表示します。長い欠測区間を含む場合、途中の操作は分かりません。単位はMiBです。</p><table><tr><th>区間終端 / 秒</th><th>RSS差</th><th>arena使用中差</th><th>arena空き差</th><th>mmap差</th><th>イベント・状態</th></tr>')
    pairs = [(a,b) for a,b in zip(rows,rows[1:]) if (delta(a,b,"process.rss_kib") or 0)>0]
    pairs.sort(key=lambda p: delta(*p,"process.rss_kib"), reverse=True)
    for a,b in pairs[:20]:
        state = b["record"].get("snapshot", {})
        flags = ", ".join(f"{k}={state[k]}" for k in ["playing", "subtitles", "comments_enabled", "epg_enabled", "guide_open"] if k in state)
        seconds = (b["record"]["unix_ms"]-a["record"]["unix_ms"])/1000
        content.append(f'<tr><td>{time_label(b)} / {seconds:.1f}</td>' + ''.join(f'<td>{number(delta(a,b,k,d))}</td>' for k,d in [("process.rss_kib",1024),("allocator.in_use_bytes",1048576),("allocator.free_bytes",1048576),("allocator.mmap_bytes",1048576)]) + f'<td>{html.escape(b["record"].get("event", ""))}<br>{html.escape(flags)}</td></tr>')
    content.append('</table><h2>操作・更新イベント</h2><p>直近500件。全記録は同梱のsourcesに保存されています。</p><table><tr><th>時刻（UTC）</th><th>イベント</th></tr>')
    events = [r for r in rows if r["record"].get("event") != "sample"]
    for row in events[-500:]:
        content.append(f'<tr><td>{time_label(row)}</td><td>{html.escape(row["record"].get("event", ""))}</td></tr>')
    content.append('</table><p>arena空きだけが増える場合は解放済み領域の保持、使用中やmmapも増える場合は生存データやキャッシュを次に調べます。関数別の割り当てスタックは通常ログに含まれません。</p>')
    return "".join(content)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    default = Path(os.environ.get("XDG_STATE_HOME") or Path.home()/".local/state")/"mirakurun-viewer/usage"
    parser.add_argument("--input", type=Path, default=default)
    parser.add_argument("--pid", type=int, help="省略時は最新のログのPID")
    parser.add_argument("--output", type=Path, required=True, help="新規の保存先ディレクトリー")
    args = parser.parse_args()
    paths = list(args.input.glob("usage-*.jsonl")) + list((args.input/"history").glob("usage-*.jsonl"))
    if not paths:
        parser.error(f"診断ログがありません: {args.input}")
    pid = args.pid or int(max(paths, key=lambda p:p.stat().st_mtime).name.split("-")[1].split(".")[0])
    paths = [p for p in paths if p.name in (f"usage-{pid}.jsonl",f"usage-{pid}.previous.jsonl")]
    if not paths:
        parser.error(f"PID {pid} のログがありません")
    args.output.mkdir(parents=True, exist_ok=False)
    archive = args.output/"sources"
    copied = []
    for path in paths:
        dest = archive/path.relative_to(args.input)
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(path,dest)
        copied.append(dest)
    rows,gc_count,invalid = read_samples(copied)
    if not rows:
        parser.error("有効な計測点がありません。保存したsourcesを確認してください")
    # PID reuse / same-process recorder restart: never draw across elapsed reset.
    starts = [i for i in range(1,len(rows)) if rows[i]["record"]["elapsed_ms"] < rows[i-1]["record"]["elapsed_ms"]]
    if starts:
        rows = rows[starts[-1]:]
        print("同じPIDの時刻リセットを検出: 最新セッションのみ表示",file=sys.stderr)
    (args.output/"index.html").write_text(report(rows,gc_count,invalid),encoding="utf-8")
    print(args.output/"index.html")


if __name__ == "__main__":
    main()
