"""Authored TV listings for publicity; all stations, shows and people are fictional."""
from dataclasses import dataclass
from datetime import datetime, timedelta
from enum import IntEnum
from zoneinfo import ZoneInfo

JST = ZoneInfo("Asia/Tokyo")
GUIDE_DAYS = 7
MINUTES_PER_HOUR = 60
MINUTES_PER_DAY = 24 * MINUTES_PER_HOUR
MILLISECONDS_PER_MINUTE = 60_000
NETWORK_BASE = 65000
SERVICE_ID = 1
EVENT_DAY_STRIDE = 100
PROGRAM_CHANNEL_STRIDE = GUIDE_DAYS * EVENT_DAY_STRIDE


class Genre(IntEnum):
    """ARIB broad genre identifiers, matching the app's guide palette."""
    NEWS = 0
    SPORTS = 1
    INFORMATION = 2
    DRAMA = 3
    MUSIC = 4
    VARIETY = 5
    FILM = 6
    ANIMATION = 7
    DOCUMENTARY = 8
    EDUCATION = 10


@dataclass(frozen=True)
class Slot:
    minute: int
    title: str
    genre: Genre
    description: str


def listing(clock, title, genre, description=""):
    hour, minute = map(int, clock.split(":"))
    if not (0 <= hour < 24 and 0 <= minute < MINUTES_PER_HOUR):
        raise ValueError(f"Invalid schedule time: {clock}")
    return Slot(hour * MINUTES_PER_HOUR + minute, title, genre, description)


# Each row starts when the preceding row ends. The next midnight closes the
# final program. Different lengths and start times are intentional editorial data.
SCHEDULES = (
    (
        listing("00:00", "[字]レイトシネマ『雨のあとで』", Genre.FILM),
        listing("01:50", "短編映画セレクション #24", Genre.FILM),
        listing("03:00", "スクリーン紀行　世界の街角", Genre.DOCUMENTARY),
        listing("05:00", "映画音楽のある朝", Genre.MUSIC),
        listing("06:00", "[字]モーニングシネマ『小さな灯台』", Genre.FILM),
        listing("07:45", "今週のシネマガイド", Genre.INFORMATION),
        listing("08:00", "[字]映画の舞台を訪ねて　海辺の町と古い駅舎", Genre.DOCUMENTARY),
        listing("08:45", "シネマ便り　新作紹介・監督インタビュー", Genre.INFORMATION),
        listing("09:00", "[字]週末アニメシアター『森の小さな物語』", Genre.ANIMATION,
                "緑あふれる森を舞台に、大きなウサギと小さな動物たちの一日を描く短編特集。"
                "▽オープンムービーの世界▽アニメーション制作の舞台裏\n作品：Big Buck Bunny／Blender Foundation"),
        listing("10:30", "[字]映画をつくる人たち #12　音で描く世界", Genre.DOCUMENTARY,
                "足音、風の音、衣擦れ。身近な道具から映画の音が生まれるまでを、音響スタッフの仕事場で追う。"),
        listing("11:00", "[字]名作映画館『夏の停留所』", Genre.FILM,
                "故郷に戻った青年が、廃線を控えたバス路線で出会う人々。ひと夏の再会を描くヒューマンドラマ。"),
        listing("12:50", "午後の映画案内", Genre.INFORMATION),
        listing("13:00", "[字]サタデーシネマ『遠い約束』", Genre.FILM),
        listing("14:50", "短編アニメーションの時間", Genre.ANIMATION),
        listing("15:15", "[字]シネマ・アーカイブ『風の図書館』", Genre.FILM),
        listing("17:00", "映画をつくる人たち #13", Genre.DOCUMENTARY),
        listing("17:30", "週末シネマニュース", Genre.INFORMATION),
        listing("18:00", "[字]ファミリーシアター『星を探す旅』", Genre.FILM),
        listing("19:50", "このあと夜のプレミアムシネマ", Genre.INFORMATION),
        listing("20:00", "[字]プレミアムシネマ『坂道の向こう』", Genre.FILM),
        listing("22:00", "[字]監督たちの仕事　物語が生まれる場所", Genre.DOCUMENTARY),
        listing("23:00", "深夜の短編映画館", Genre.FILM),
    ),
    (
        listing("00:00", "[再]夜のトーク便　旅する本棚", Genre.VARIETY),
        listing("01:00", "ミュージック・ナイト", Genre.MUSIC),
        listing("04:00", "朝の風景　四季の日本", Genre.DOCUMENTARY),
        listing("05:30", "おはようニュース・天気", Genre.NEWS),
        listing("06:00", "そよかぜモーニング　きょうのニュース", Genre.NEWS),
        listing("07:00", "[字]週末おでかけナビ　秋の行楽特集", Genre.INFORMATION),
        listing("08:00", "[字]土曜の朝いち！▽新米のおいしい炊き方", Genre.INFORMATION),
        listing("09:25", "ニュース・天気・交通情報", Genre.NEWS),
        listing("09:40", "[字]ぶらり沿線さんぽ　港町の商店街へ", Genre.VARIETY,
                "路面電車で巡る港町。朝市で見つけた旬の味と、親子三代で守るパン店を訪ねる。"
                "▽途中下車で出会う職人技▽路地裏の喫茶店"),
        listing("10:35", "[字]おうち時間ラボ　片づけが続く収納術", Genre.INFORMATION),
        listing("11:25", "お昼のニュース", Genre.NEWS),
        listing("11:45", "[字]週末キッチン　きのこの炊き込みごはん", Genre.INFORMATION),
        listing("12:15", "[字]みんなのスポーツ　地域クラブの挑戦", Genre.SPORTS),
        listing("13:00", "[字]土曜スペシャル　ローカル線で小さな旅", Genre.VARIETY),
        listing("14:55", "午後のニュース・天気", Genre.NEWS),
        listing("15:10", "[再]連続ドラマ『坂の途中で』第3話", Genre.DRAMA),
        listing("16:05", "そよかぜショッピング", Genre.INFORMATION),
        listing("16:30", "[字]どうぶつ日和　保護猫カフェの一日", Genre.VARIETY),
        listing("17:25", "週末ニュース・気象情報", Genre.NEWS),
        listing("18:00", "[字]街の食卓　家族で営む洋食店", Genre.VARIETY),
        listing("19:00", "[字]発見！わたしたちの町", Genre.VARIETY),
        listing("20:00", "[字]土曜ドラマ『雨上がりの手紙』第6話", Genre.DRAMA),
        listing("20:55", "そよかぜニュース", Genre.NEWS),
        listing("21:10", "[字]旅する週末　山あいの温泉街", Genre.VARIETY),
        listing("22:00", "ニュース・ウィークエンド", Genre.NEWS),
        listing("23:00", "夜のトーク便　好きな一冊", Genre.VARIETY),
    ),
    (
        listing("00:00", "[再]暮らしの手帖　古い家を楽しむ", Genre.INFORMATION),
        listing("01:00", "世界の台所から", Genre.DOCUMENTARY),
        listing("04:00", "やさしい朝の体操", Genre.INFORMATION),
        listing("05:00", "季節の花だより", Genre.INFORMATION),
        listing("06:00", "[字]はじめての家庭菜園　秋まき野菜", Genre.EDUCATION),
        listing("07:00", "[字]朝ごはんの時間　具だくさんスープ", Genre.INFORMATION),
        listing("07:30", "暮らしのショッピング", Genre.INFORMATION),
        listing("08:00", "[再][字]小さな庭と暮らす #8　ハーブのある窓辺", Genre.EDUCATION),
        listing("08:30", "[字]わたしの台所　道具を長く使う工夫", Genre.INFORMATION),
        listing("09:00", "[字]季節の食卓　秋なすときのこの献立", Genre.INFORMATION,
                "旬の野菜で作る、一汁二菜の秋ごはん。なすの焼きびたしときのこの蒸しものを紹介。"
                "▽下ごしらえのこつ▽翌日もおいしい保存方法"),
        listing("09:30", "[字]手作りの時間　布で作る小さなバッグ", Genre.EDUCATION),
        listing("09:55", "暮らしのミニ知識　衣替えの前に", Genre.INFORMATION),
        listing("10:10", "[字]住まいの相談室　光と風を取り込む間取り", Genre.INFORMATION),
        listing("10:55", "くらしのお知らせ", Genre.INFORMATION),
        listing("11:10", "[字]週末の作りおき　お弁当にも使える3品", Genre.INFORMATION),
        listing("11:40", "[字]親子でおやつ　さつまいもの蒸しパン", Genre.INFORMATION),
        listing("12:00", "くらしセレクション", Genre.INFORMATION),
        listing("12:30", "[再][字]古い家を楽しむ　築80年の台所改修", Genre.DOCUMENTARY),
        listing("13:25", "花と暮らす　秋の枝もの", Genre.EDUCATION),
        listing("14:00", "[字]手しごと工房　器を繕う", Genre.EDUCATION),
        listing("15:00", "午後のショッピング", Genre.INFORMATION),
        listing("16:00", "[字]わが家の防災　備蓄を見直す", Genre.INFORMATION),
        listing("17:00", "[字]今夜の献立　魚のホイル焼き", Genre.INFORMATION),
        listing("18:00", "[字]小さな庭と暮らす #9", Genre.EDUCATION),
        listing("19:00", "[字]暮らしの手帖　ものを減らして豊かに", Genre.DOCUMENTARY),
        listing("20:00", "[字]世界の台所から　港町の家庭料理", Genre.DOCUMENTARY),
        listing("21:00", "[字]住まいの相談室　リフォーム特集", Genre.INFORMATION),
        listing("22:00", "[再]季節の食卓・週末の作りおき", Genre.INFORMATION),
        listing("23:00", "音楽と花のある夜", Genre.MUSIC),
    ),
    (
        listing("00:00", "深夜のアートシアター", Genre.FILM),
        listing("02:00", "音楽の風景　ピアノ名曲集", Genre.MUSIC),
        listing("05:00", "朝のギャラリー", Genre.DOCUMENTARY),
        listing("06:00", "[字]名画への招待　光を描く画家たち", Genre.EDUCATION),
        listing("07:00", "[字]はじめてのデッサン 第4回", Genre.EDUCATION),
        listing("07:30", "[再]音のある休日　弦楽四重奏", Genre.MUSIC),
        listing("08:30", "[字]手しごとの時間　木のスプーンを削る", Genre.EDUCATION),
        listing("09:00", "[字]アトリエ訪問 #18　町の小さな活版印刷所", Genre.DOCUMENTARY,
                "一文字ずつ活字を拾い、紙に思いを刷る。昔ながらの印刷所で働く職人と、"
                "新しい表現に挑む若いデザイナーの一日を訪ねる。"),
        listing("09:50", "今週の展覧会ガイド", Genre.INFORMATION),
        listing("10:05", "[字]名画のひみつ　窓辺に差す光", Genre.EDUCATION),
        listing("10:35", "[再]陶芸はじめの一歩 第6回　釉薬を選ぶ", Genre.EDUCATION),
        listing("11:00", "街角ライブ　週末ジャズセッション", Genre.MUSIC),
        listing("11:55", "アート・トピックス", Genre.INFORMATION),
        listing("12:10", "[字]建築を歩く　受け継がれる木造校舎", Genre.DOCUMENTARY),
        listing("13:00", "ウィークエンド・コンサート　室内楽の午後", Genre.MUSIC),
        listing("14:30", "[字]舞台のしごと　照明でつくる空間", Genre.DOCUMENTARY),
        listing("15:30", "[再]アトリエ訪問　ガラス工房", Genre.DOCUMENTARY),
        listing("16:20", "展覧会ガイド", Genre.INFORMATION),
        listing("16:40", "[字]手しごとの時間　和紙の照明", Genre.EDUCATION),
        listing("17:10", "[字]写真で綴る旅　海辺の町", Genre.DOCUMENTARY),
        listing("18:00", "[字]名画への招待　印象派の時代", Genre.EDUCATION),
        listing("19:00", "土曜クラシック　オーケストラの響き", Genre.MUSIC),
        listing("21:00", "[字]表現者たち　ダンサーの身体", Genre.DOCUMENTARY),
        listing("22:00", "ナイト・ジャズ", Genre.MUSIC),
        listing("23:00", "[再]建築を歩く　夜の美術館", Genre.DOCUMENTARY),
    ),
    (
        listing("00:00", "[再]宇宙への窓　銀河の旅", Genre.DOCUMENTARY),
        listing("02:00", "地球の記録　海と大気", Genre.DOCUMENTARY),
        listing("05:00", "サイエンス・アーカイブ", Genre.EDUCATION),
        listing("06:00", "[字]身近なふしぎ　水の力", Genre.EDUCATION),
        listing("07:00", "[字]こども実験室　紙ひこうきを飛ばそう", Genre.EDUCATION),
        listing("07:30", "[字]いきもの観察ノート　朝の公園", Genre.DOCUMENTARY),
        listing("08:00", "[字]地球フィールドワーク　森の土の中", Genre.DOCUMENTARY),
        listing("08:50", "今週の科学ニュース", Genre.NEWS),
        listing("09:05", "[字]サイエンス・ラボ　光と色のふしぎ", Genre.EDUCATION,
                "虹はどうして七色に見える？身近な道具を使って、光が分かれる仕組みを実験。"
                "▽シャボン玉の色▽空が青く見える理由"),
        listing("09:35", "[字]未来のものづくり #7　町工場の小さなロボット", Genre.DOCUMENTARY),
        listing("10:20", "こども実験室　磁石であそぼう", Genre.EDUCATION),
        listing("10:45", "[字]星空案内　秋の星座と月", Genre.EDUCATION),
        listing("11:00", "[字]宇宙への窓　惑星探査の最前線", Genre.DOCUMENTARY),
        listing("11:50", "科学ニュース・ピックアップ", Genre.NEWS),
        listing("12:05", "[再][字]いきもの観察ノート　干潟の一日", Genre.DOCUMENTARY),
        listing("12:35", "[字]数の世界へ　第5回・かたちと規則", Genre.EDUCATION),
        listing("13:00", "[字]特集・海の研究者たち　深海に挑む", Genre.DOCUMENTARY),
        listing("14:30", "[字]こども実験室　親子で自由研究", Genre.EDUCATION),
        listing("15:00", "[再]未来のものづくり　新しい電池", Genre.DOCUMENTARY),
        listing("16:00", "[字]地球フィールドワーク　火山の島", Genre.DOCUMENTARY),
        listing("17:00", "サイエンス・ラボ傑作選", Genre.EDUCATION),
        listing("18:00", "週刊サイエンスニュース", Genre.NEWS),
        listing("18:30", "[字]いきもの観察ノート　渡り鳥の季節", Genre.DOCUMENTARY),
        listing("19:00", "[字]科学ドキュメント　新しい望遠鏡", Genre.DOCUMENTARY),
        listing("20:00", "[字]宇宙への窓　星の誕生", Genre.DOCUMENTARY),
        listing("21:00", "[字]地球の記録　極地の一年", Genre.DOCUMENTARY),
        listing("22:00", "[再]身近なふしぎ・数の世界へ", Genre.EDUCATION),
        listing("23:00", "星空案内・夜の天文台", Genre.DOCUMENTARY),
    ),
    (
        listing("00:00", "[再]まちかどステージ　市民音楽祭", Genre.MUSIC),
        listing("01:30", "まちの風景アーカイブ", Genre.DOCUMENTARY),
        listing("05:00", "おはよう地域情報", Genre.NEWS),
        listing("06:00", "まちかど朝便　天気・交通・暮らしの情報", Genre.NEWS),
        listing("07:00", "[字]朝市めぐり　とれたて野菜と地魚", Genre.INFORMATION),
        listing("07:30", "[字]わたしの街の物語　港を支える仕事", Genre.DOCUMENTARY),
        listing("08:00", "週末まちナビ▽お祭り・スポーツ・展覧会", Genre.INFORMATION),
        listing("08:30", "[字]ローカルさんぽ　旧街道の喫茶店", Genre.VARIETY),
        listing("09:15", "まちかどニュース　今週の出来事", Genre.NEWS,
                "地域の一週間を振り返る。新しい図書館が開館▽商店街の秋祭りの準備"
                "▽週末の天気と交通情報。"),
        listing("09:45", "[字]わたしの街の物語　創業70年のパン屋", Genre.DOCUMENTARY),
        listing("10:15", "[字]わくわく放課後　科学クラブに密着", Genre.EDUCATION),
        listing("10:45", "週末イベント案内", Genre.INFORMATION),
        listing("11:00", "[生]地域スポーツ　少年サッカー決勝", Genre.SPORTS,
                "市民スポーツ公園から生中継。青空ジュニア×港南キッズ。選手たちの夏の成長と、"
                "チームを支える家族の声を紹介。"),
        listing("12:30", "お昼のまちかどニュース", Genre.NEWS),
        listing("12:50", "[字]まちの食堂　商店街の定食屋さん", Genre.INFORMATION),
        listing("13:20", "[再]ローカルさんぽ　水辺の遊歩道", Genre.VARIETY),
        listing("14:05", "わが街チャンネル　市民からのおたより", Genre.INFORMATION),
        listing("15:00", "まちかどステージ　市民音楽祭", Genre.MUSIC),
        listing("16:30", "[字]地域の防災　いざという時の避難", Genre.INFORMATION),
        listing("17:00", "まちかどニュース・天気", Genre.NEWS),
        listing("17:30", "[字]わたしの街の物語　小さな書店", Genre.DOCUMENTARY),
        listing("18:00", "[字]ふるさとの台所　受け継がれる味", Genre.INFORMATION),
        listing("19:00", "[再]地域スポーツ　少年サッカー決勝", Genre.SPORTS),
        listing("20:30", "夜のまちかどニュース", Genre.NEWS),
        listing("21:00", "[字]地域ドキュメント　最後の渡し船", Genre.DOCUMENTARY),
        listing("22:00", "[再]週末まちナビ", Genre.INFORMATION),
        listing("23:00", "まちの夜景・週末の天気", Genre.INFORMATION),
    ),
)

# The offscreen BS tabs have complete schedules as well.
SCHEDULES += (SCHEDULES[0], SCHEDULES[1])


def programs(now):
    midnight = now.astimezone(JST).replace(hour=0, minute=0, second=0, microsecond=0)
    entries = []
    for channel, slots in enumerate(SCHEDULES):
        starts = tuple(slot.minute for slot in slots)
        if not starts or starts[0] != 0 or any(a >= b for a, b in zip(starts, starts[1:])):
            raise ValueError(f"Channel {channel} must have ordered, contiguous daily listings")
        ends = (*starts[1:], MINUTES_PER_DAY)
        for day in range(GUIDE_DAYS):
            for index, (slot, end) in enumerate(zip(slots, ends)):
                start = midnight + timedelta(days=day, minutes=slot.minute)
                event_id = day * EVENT_DAY_STRIDE + index + 1
                entries.append(dict(
                    id=channel * PROGRAM_CHANNEL_STRIDE + event_id,
                    eventId=event_id,
                    networkId=NETWORK_BASE + channel,
                    serviceId=SERVICE_ID,
                    startAt=int(start.timestamp() * 1000),
                    duration=(end - slot.minute) * MILLISECONDS_PER_MINUTE,
                    name=slot.title,
                    description=slot.description,
                    genres=[dict(lv1=slot.genre, lv2=0)],
                ))
    return entries


def on_air_pair(entries, now):
    """Current and next station-one events, also used to author matching TS SI."""
    now_ms = int(now.timestamp() * 1000)
    station = [entry for entry in entries if entry["networkId"] == NETWORK_BASE]
    for index, entry in enumerate(station):
        if entry["startAt"] <= now_ms < entry["startAt"] + entry["duration"]:
            return entry, station[index + 1]
    raise ValueError("No current and next demo programs")
