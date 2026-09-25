//! HTTP fixtures for the production-window tests; no display or media devices.
//! Catalogue/channel JSON was captured from the pinned real EPGStation provider.
//! scripts/epgstation-integration.py checks for drift and verifies file responses.
use std::{
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate, matchers::any};

pub(super) const LIBRARY_RECORDINGS: usize = 12;

pub(super) struct Server {
    mock: MockServer,
    requests: Arc<AtomicUsize>,
}

impl Server {
    pub(super) fn new() -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        // Wiremock owns its HTTP runtime and isolates each connection. In particular,
        // responses run only after the complete request body has been received.
        let mock = runtime.block_on(
            MockServer::builder()
                .listener(listener)
                .disable_request_recording()
                .start(),
        );
        let requests = Arc::new(AtomicUsize::new(0));
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(std::io::Error::other)?;
        let programs = serde_json::json!([{
            "id":101, "eventId":7, "networkId":10, "serviceId":2,
            "startAt": now.as_secs().saturating_sub(60) * 1000, "duration":3_600_000,
            "name":"Metadata program", "description":"Program overview", "isFree":true,
            "genres":[{"lv1":3,"lv2":0}],
            "extended":{"番組内容":"詳しい番組内容", "出演者":"出演者テスト", "スタッフ":"スタッフテスト"},
            "video":{"type":"mpeg2", "resolution":"1080i"},
            "audios":[{"componentTag":16,"componentType":3,"isMain":true,"langs":["jpn"],"samplingRate":48000}],
            "series":{"name":"Test series","episode":3,"lastEpisode":12}
        }]).to_string();
        let recordings: serde_json::Value = crate::json::from_slice(include_bytes!(
            "../../../../tests/fixtures/epgstation/recorded.json"
        ))
        .map_err(std::io::Error::other)?;
        runtime.block_on(
            Mock::given(any())
                .respond_with(Responses {
                    programs,
                    recordings,
                    requests: requests.clone(),
                })
                .mount(&mock),
        );
        Ok(Self { mock, requests })
    }

    pub(super) fn url(&self) -> String {
        self.mock.uri()
    }

    pub(super) fn requests(&self) -> usize {
        self.requests.load(Ordering::Relaxed)
    }
}

struct Responses {
    programs: String,
    recordings: serde_json::Value,
    requests: Arc<AtomicUsize>,
}

impl Respond for Responses {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let path = request.url.path();
        if request.method == "POST" && path == "/protected/api/auth/login" {
            // Invalid credentials are an intentional fixture response. Never retain
            // or print login bodies in mock diagnostics.
            return match request.body_json::<serde_json::Value>() {
                Ok(credentials)
                    if credentials
                        == serde_json::json!({"name":"viewer", "password":"fixture-password"}) =>
                {
                    ResponseTemplate::new(200)
                        .insert_header(
                            "set-cookie",
                            "epgstation_session=fixture; Path=/protected/; HttpOnly; SameSite=Lax",
                        )
                        .set_body_json(serde_json::json!({"user":{"id":1,"name":"viewer"}}))
                }
                Ok(_) | Err(_) => ResponseTemplate::new(401),
            };
        }
        let has_cookie = request
            .headers
            .get("cookie")
            .is_some_and(|value| value == "epgstation_session=fixture");
        let api_path = path.strip_prefix("/protected").unwrap_or(path);
        // A nonnumeric suffix may be a metadata route; otherwise it reaches the
        // explicit 404 below. Parsing failure must never become a success body.
        let video_id = api_path
            .strip_prefix("/api/videos/")
            .and_then(|id| id.parse::<u64>().ok());
        let has_media_token = request.method == "GET"
            && video_id.is_some()
            && request
                .url
                .query_pairs()
                .any(|(key, value)| key == "token" && value == "playback-fixture");
        if path.starts_with("/protected/api/") && !has_cookie && !has_media_token {
            return ResponseTemplate::new(401);
        }
        if request.method != "GET" {
            return ResponseTemplate::new(404);
        }
        if let Some(video_id) = video_id {
            for row in self.recordings["records"]
                .as_array()
                .expect("captured records")
            {
                for video in row["videoFiles"].as_array().expect("captured files") {
                    if video["id"].as_u64() == Some(video_id) {
                        let (bytes, mime) = match video["filename"].as_str() {
                            Some("recording-seek.ts") => (
                                include_bytes!("../../../../tests/fixtures/recording-seek.ts")
                                    .as_slice(),
                                "video/mp2t",
                            ),
                            Some("media-h264.mp4") => (
                                include_bytes!("../../../../tests/fixtures/media-h264.mp4")
                                    .as_slice(),
                                "video/mp4",
                            ),
                            Some("media-hevc.mkv") => (
                                include_bytes!("../../../../tests/fixtures/media-hevc.mkv")
                                    .as_slice(),
                                "video/matroska",
                            ),
                            _ => return ResponseTemplate::new(500),
                        };
                        return video_response(request, bytes, mime);
                    }
                }
            }
            return ResponseTemplate::new(404);
        }
        let body = match api_path {
            "/api/services" => {
                self.requests.fetch_add(1, Ordering::Relaxed);
                r#"[{"id":1,"networkId":10,"serviceId":1,"name":"First TV","type":1,"channel":{"type":"GR"}},
                    {"id":2,"networkId":10,"serviceId":2,"name":"Saved TV","type":1,"channel":{"type":"GR"}}]"#
            }
            "/api/programs" => &self.programs,
            "/api/recorded" => return catalogue_response(request, &self.recordings),
            "/api/channels" => include_str!("../../../../tests/fixtures/epgstation/channels.json"),
            "/api/videos/124/metadata" => {
                include_str!("../../../../tests/fixtures/epgstation/video-124-metadata.json")
            }
            "/api/auth/media-token" if has_cookie => r#"{"token":"playback-fixture"}"#,
            "/api/services/2/stream" => return ResponseTemplate::new(503),
            _ => return ResponseTemplate::new(404),
        };
        ResponseTemplate::new(200).set_body_raw(body, "application/json")
    }
}

fn catalogue_response(request: &Request, catalogue: &serde_json::Value) -> ResponseTemplate {
    let query: std::collections::HashMap<_, _> = request.url.query_pairs().collect();
    if query
        .keys()
        .any(|key| !matches!(key.as_ref(), "isHalfWidth" | "offset" | "limit" | "keyword"))
        || query
            .get("isHalfWidth")
            .is_none_or(|value| value != "false")
    {
        return ResponseTemplate::new(400);
    }
    let number = |key, default| match query.get(key) {
        Some(value) => value.parse::<usize>(),
        None => Ok(default),
    };
    let (Ok(offset), Ok(limit)) = (number("offset", 0), number("limit", 50)) else {
        return ResponseTemplate::new(400);
    };
    let keyword = query.get("keyword").map_or("", |value| value.as_ref());
    let records: Vec<_> = catalogue["records"]
        .as_array()
        .expect("captured records")
        .iter()
        .filter(|row| {
            row["name"]
                .as_str()
                .expect("captured name")
                .contains(keyword)
        })
        .collect();
    ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "total": records.len(), "records": records.into_iter().skip(offset).take(limit).collect::<Vec<_>>()
    }))
}

fn video_response(request: &Request, bytes: &[u8], mime: &str) -> ResponseTemplate {
    let Some(range) = request.headers.get("range") else {
        return ResponseTemplate::new(200).set_body_raw(bytes.to_vec(), mime);
    };
    let parsed = range
        .to_str()
        .ok()
        .and_then(|range| range.strip_prefix("bytes="))
        .and_then(|range| range.split_once('-'))
        .and_then(|(start, end)| {
            if start.is_empty() {
                let length = end.parse::<usize>().ok().filter(|length| *length > 0)?;
                Some((bytes.len().saturating_sub(length), bytes.len() - 1))
            } else {
                Some((
                    start.parse::<usize>().ok()?,
                    if end.is_empty() {
                        bytes.len() - 1
                    } else {
                        end.parse().ok()?
                    },
                ))
            }
        });
    // Deliberate parse-to-HTTP-error conversion. Invalid/unsupported ranges are
    // normal protocol inputs, not operational failures. Pinned provider rejects
    // an end past EOF (HttpRangeUtil.ts); the live differential test checks this.
    match parsed {
        Some((start, end)) if start <= end && end < bytes.len() => ResponseTemplate::new(206)
            .insert_header(
                "content-range",
                format!("bytes {start}-{end}/{}", bytes.len()),
            )
            .set_body_raw(bytes[start..=end].to_vec(), mime),
        _ => ResponseTemplate::new(416),
    }
}

#[cfg(test)]
#[path = "server/tests.rs"]
mod tests;
