//! One policy boundary for gRPC, gRPC-Web and embedded browser assets.
#![forbid(unsafe_code)]

use http::{Method, Request, Response, StatusCode, header};
use std::{
    future::Future,
    net::{Ipv4Addr, Ipv6Addr},
    pin::Pin,
    task::{Context, Poll},
};
use tonic::body::Body;
use tower::{Layer, Service};

pub(crate) struct Asset {
    path: &'static str,
    contents: &'static [u8],
    content_type: &'static str,
    immutable: bool,
}
include!(concat!(env!("OUT_DIR"), "/web_assets.rs"));

#[derive(Clone)]
pub(crate) struct HttpLayer {
    pub web_enabled: bool,
}
impl<S> Layer<S> for HttpLayer {
    type Service = HttpService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        HttpService {
            inner,
            web_enabled: self.web_enabled,
        }
    }
}
#[derive(Clone)]
pub(crate) struct HttpService<S> {
    inner: S,
    web_enabled: bool,
}
impl<S> Service<Request<Body>> for HttpService<S>
where
    S: Service<Request<Body>, Response = Response<Body>>,
    S::Future: Send + 'static,
    S::Error: 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }
    fn call(&mut self, request: Request<Body>) -> Self::Future {
        if !allowed_request(&request) {
            // Do not log arbitrary attacker-controlled header contents.
            tracing::warn!("Remote HTTP request rejected: missing or disallowed Host/authority");
            return Box::pin(async { Ok(response(StatusCode::FORBIDDEN)) });
        }
        let path = request.uri().path();
        if request.method() == Method::GET || request.method() == Method::HEAD {
            let result = if self.web_enabled {
                asset_response(path, request.method() == Method::HEAD)
            } else {
                response(StatusCode::NOT_FOUND)
            };
            return Box::pin(async { Ok(result) });
        }
        if !path.starts_with("/viewer.v1.PlayerService/") {
            return Box::pin(async { Ok(response(StatusCode::NOT_FOUND)) });
        }
        Box::pin(self.inner.call(request))
    }
}
fn response(status: StatusCode) -> Response<Body> {
    Response::builder()
        .status(status)
        .body(Body::empty())
        .expect("static response")
}
fn asset_response(path: &str, head: bool) -> Response<Body> {
    let path = if path == "/" { "/index.html" } else { path };
    let Some(asset) = ASSETS.iter().find(|asset| asset.path == path) else {
        return response(StatusCode::NOT_FOUND);
    };
    let body = if head {
        Body::empty()
    } else {
        Body::new(http_body_util::Full::new(bytes::Bytes::from_static(
            asset.contents,
        )))
    };
    Response::builder()
        .header(header::CONTENT_TYPE, asset.content_type)
        .header(header::CONTENT_LENGTH, asset.contents.len())
        .header(
            header::CACHE_CONTROL,
            if asset.immutable {
                "public, max-age=31536000, immutable"
            } else {
                "no-cache"
            },
        )
        .header("x-content-type-options", "nosniff")
        .body(body)
        .expect("static asset response")
}
fn allowed_request<B>(request: &Request<B>) -> bool {
    let mut supplied = false;
    let hosts = request.headers().get_all(header::HOST);
    // Multiple Host values are ambiguous even when each value is allowed.
    if hosts.iter().count() > 1 {
        return false;
    }
    for host in hosts {
        supplied = true;
        if !host.to_str().is_ok_and(allowed_authority) {
            return false;
        }
    }
    if let Some(authority) = request.uri().authority() {
        supplied = true;
        if !allowed_authority(authority.as_str()) {
            return false;
        }
    }
    supplied
}
fn port_allowed(suffix: &str) -> bool {
    suffix.is_empty()
        || suffix.strip_prefix(':').is_some_and(|port| {
            !port.is_empty()
                && port.bytes().all(|b| b.is_ascii_digit())
                && port.parse::<u16>().is_ok()
        })
}
fn allowed_authority(value: &str) -> bool {
    if let Some(ipv6) = value.strip_prefix('[') {
        return ipv6.split_once(']').is_some_and(|(address, suffix)| {
            address.parse::<Ipv6Addr>().is_ok() && port_allowed(suffix)
        });
    }
    let (host, suffix) = value
        .find(':')
        .map_or((value, ""), |index| value.split_at(index));
    (host.eq_ignore_ascii_case("localhost") || host.parse::<Ipv4Addr>().is_ok())
        && port_allowed(suffix)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authority_policy_rejects_names_missing_hosts_and_ambiguous_headers() {
        for host in [
            "127.0.0.1",
            "192.168.1.2:50051",
            "[::1]",
            "[2001:db8::1]:80",
            "localhost",
            "LOCALHOST:50051",
        ] {
            assert!(allowed_authority(host), "{host}");
        }
        for host in [
            "",
            "example.org",
            "127.0.0.1.evil.test",
            "localhost.",
            "localhost:",
            "localhost:65536",
            "localhost:+80",
            "localhost:abc",
            "user@localhost",
            "::1",
            "[::1%lo]",
            "[::1]evil",
            " localhost",
            "localhost ",
        ] {
            assert!(!allowed_authority(host), "{host}");
        }
        assert!(!allowed_request(&Request::new(())));
        assert!(allowed_request(
            &Request::builder()
                .uri("http://[::1]:50051/")
                .body(())
                .unwrap()
        ));
        let mut request = Request::builder()
            .uri("http://localhost/")
            .header(header::HOST, "evil.test")
            .body(())
            .unwrap();
        assert!(!allowed_request(&request));
        request
            .headers_mut()
            .insert(header::HOST, "localhost".parse().unwrap());
        request
            .headers_mut()
            .append(header::HOST, "localhost".parse().unwrap());
        assert!(!allowed_request(&request));
    }
}
