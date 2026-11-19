use super::*;
use std::net::{Ipv4Addr, SocketAddr};
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::path};

#[test]
fn scope_honors_the_real_subnet_and_the_24_bit_limit() {
    use networks::{Network, addresses};
    let parse =
        |ip: &str, mask: &str| Network::from_address(ip.parse().unwrap(), mask.parse().unwrap());

    let broad = parse("10.4.7.100", "255.255.0.0").unwrap();
    let narrow = parse("192.168.3.99", "255.255.255.224").unwrap();
    let overlapping = parse("192.168.3.118", "255.255.255.224").unwrap();
    let point_to_point = parse("172.17.0.6", "255.255.255.254").unwrap();
    let candidates = addresses(
        &[broad, narrow, overlapping, point_to_point]
            .into_iter()
            .collect(),
    );
    assert_eq!(candidates.first(), Some(&Ipv4Addr::LOCALHOST));
    assert_eq!(candidates.len(), 1 + 256 + 30 + 2);
    assert!(candidates[1..].iter().all(Ipv4Addr::is_private));
    let includes = |ip: &str| candidates.contains(&ip.parse().unwrap());
    for ip in [
        "10.4.7.0",
        "10.4.7.255",
        "192.168.3.97",
        "192.168.3.126",
        "172.17.0.6",
        "172.17.0.7",
    ] {
        assert!(includes(ip), "{ip}");
    }
    for ip in [
        "10.4.6.255",
        "10.4.8.0",
        "192.168.3.96",
        "192.168.3.127",
        "172.17.0.8",
    ] {
        assert!(!includes(ip), "{ip}");
    }
    for ip in ["1.2.3.4", "169.254.12.1", "100.64.0.1", "127.0.0.1"] {
        assert!(parse(ip, "255.255.0.0").is_none());
    }
    assert!(parse("10.4.0.1", "255.0.255.0").is_none());
}

fn address(server: &MockServer) -> SocketAddrV4 {
    let SocketAddr::V4(address) = *server.address() else {
        panic!("wiremock fixture must use IPv4 loopback");
    };
    assert!(address.ip().is_loopback());
    address
}

#[tokio::test]
async fn finds_known_tuners_and_proxied_compatible_catalogues()
-> Result<(), Box<dyn std::error::Error>> {
    let client = client()?;
    for (header, name, version) in [
        (Some("Mirakurun/4.1.3"), "Mirakurun", "4.1.3"),
        (Some("mirakc/3.4.89"), "mirakc", "3.4.89"),
        (None, "Mirakurun / mirakc", "5.0.0-beta.1"),
    ] {
        let server = MockServer::start().await;
        let mut services = ResponseTemplate::new(200).set_body_string("[]");
        if let Some(header) = header {
            services = services.insert_header("Server", header);
        }
        Mock::given(path("/api/services"))
            .respond_with(services)
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(path("/api/version"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(r#"{"current":"5.0.0-beta.1"}"#),
            )
            .expect(u64::from(header.is_none()))
            .mount(&server)
            .await;
        let results = scan(&client, Kind::Tuner, vec![address(&server)], false).await;
        assert!(!results.partial);
        assert_eq!(
            results.servers,
            vec![Server {
                url: server.uri(),
                name,
                version: Some(version.into()),
            }]
        );
    }
    Ok(())
}

#[tokio::test]
async fn validates_epgstation_catalogue_and_reads_version() -> Result<(), Box<dyn std::error::Error>>
{
    let client = client()?;
    let server = MockServer::start().await;
    Mock::given(path("/api/recorded"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"records":[],"total":0}"#))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/api/version"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"version":"2.10.0"}"#))
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        probe(&client, Kind::Recordings, address(&server)).await?,
        Server {
            url: server.uri(),
            name: "EPGStation",
            version: Some("2.10.0".into()),
        }
    );
    Ok(())
}

#[tokio::test]
async fn excludes_nonservices_and_never_follows_redirects() -> Result<(), Box<dyn std::error::Error>>
{
    let client = client()?;
    let unrelated = MockServer::start().await;
    Mock::given(path("/api/services"))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .expect(1)
        .mount(&unrelated)
        .await;
    Mock::given(path("/api/recorded"))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .expect(1)
        .mount(&unrelated)
        .await;
    Mock::given(path("/outside"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&unrelated)
        .await;
    let redirect = MockServer::start().await;
    Mock::given(path("/api/services"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("Location", format!("{}/outside", unrelated.uri())),
        )
        .expect(1)
        .mount(&redirect)
        .await;

    let results = scan(
        &client,
        Kind::Tuner,
        vec![address(&unrelated), address(&redirect)],
        false,
    )
    .await;
    assert!(results.servers.is_empty());
    assert!(!results.partial);
    assert!(
        probe(&client, Kind::Recordings, address(&unrelated))
            .await
            .is_err()
    );
    Ok(())
}

#[test]
fn cancellation_discards_a_queued_result_and_clears_the_view()
-> Result<(), Box<dyn std::error::Error>> {
    let network = Network::new()?;
    let job = network.job(async {
        Ok(Results {
            servers: vec![Server {
                url: "http://127.0.0.1".into(),
                name: "mirakc",
                version: None,
            }],
            partial: false,
        })
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while !job.is_finished() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    let mut search = Search(State::Searching(Kind::Tuner, job));
    search.cancel(Kind::Tuner);
    assert!(!search.poll(&network));
    assert_eq!(search.kind(), None);
    assert_eq!(search.status(), "idle");
    assert!(search.results().is_empty());
    Ok(())
}
