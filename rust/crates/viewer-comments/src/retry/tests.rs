use super::*;
use reqwest::StatusCode;

const ENDPOINT: &str = "https://nx.example/threads";
const JITTER: Duration = Duration::from_secs(17);
const BEFORE_DEADLINE: Duration = Duration::from_millis(1);

fn waiting(policy: &Policy, operation: Operation, now: Instant) -> Instant {
    match policy.check(operation, ENDPOINT, now) {
        Err(Blocked::Waiting(until)) => until,
        other => panic!("expected deadline: {other:?}"),
    }
}

#[test]
fn failures_without_headers_back_off_to_a_cap_in_each_lane() {
    for (operation, seconds) in [
        (Operation::Live, vec![5, 10, 20, 40, 80, 160, 300, 300]),
        (Operation::Activity, vec![60, 120, 240, 300, 300]),
    ] {
        let mut policy = Policy::default();
        let mut now = Instant::now();
        for delay in seconds {
            policy.failed_with_jitter(
                operation,
                ENDPOINT,
                Failure::Temporary,
                "offline".into(),
                now,
                JITTER,
            );
            let until = waiting(&policy, operation, now);
            assert_eq!(until - now, Duration::from_secs(delay) + JITTER);
            assert!(
                policy
                    .check(operation, ENDPOINT, until - BEFORE_DEADLINE)
                    .is_err()
            );
            assert!(policy.check(operation, ENDPOINT, until).is_ok());
            now = until;
        }
    }
}

#[test]
fn throttling_without_headers_is_shared_and_success_cannot_shorten_it() {
    let mut policy = Policy::default();
    let now = Instant::now();
    policy.failed_with_jitter(
        Operation::Live,
        ENDPOINT,
        Failure::Http {
            status: StatusCode::TOO_MANY_REQUESTS,
            retry_after: None,
        },
        "limited".into(),
        now,
        JITTER,
    );
    let until = now + RATE_LIMIT_DELAY + JITTER;
    policy.activity_succeeded(now);
    policy.stable_live();
    policy.posting_succeeded();
    for operation in [Operation::Live, Operation::Activity, Operation::Posting] {
        assert_eq!(waiting(&policy, operation, now), until);
        assert!(
            policy
                .check(operation, "another channel", until - BEFORE_DEADLINE)
                .is_err()
        );
        assert!(policy.check(operation, ENDPOINT, until).is_ok());
    }
}

#[test]
fn jitter_follows_the_later_of_backoff_and_server_deadline() {
    for server_delay in [Duration::ZERO, Duration::from_secs(3600)] {
        let mut policy = Policy::default();
        let now = Instant::now();
        policy.failed_with_jitter(
            Operation::Activity,
            ENDPOINT,
            Failure::Http {
                status: StatusCode::SERVICE_UNAVAILABLE,
                retry_after: Some(server_delay),
            },
            "unavailable".into(),
            now,
            JITTER,
        );
        let until = now + server_delay.max(ACTIVITY_INTERVAL) + JITTER;
        for operation in [Operation::Live, Operation::Activity, Operation::Posting] {
            assert_eq!(waiting(&policy, operation, now), until);
        }
    }
}

#[test]
fn success_resets_only_its_own_failure_streak() {
    let mut policy = Policy::default();
    let mut now = Instant::now();
    for _ in 0..3 {
        policy.failed_with_jitter(
            Operation::Live,
            ENDPOINT,
            Failure::Temporary,
            "offline".into(),
            now,
            Duration::ZERO,
        );
        now = waiting(&policy, Operation::Live, now);
    }
    policy.activity_succeeded(now);
    policy.failed_with_jitter(
        Operation::Live,
        ENDPOINT,
        Failure::Temporary,
        "offline".into(),
        now,
        Duration::ZERO,
    );
    assert_eq!(
        waiting(&policy, Operation::Live, now) - now,
        Duration::from_secs(40)
    );
    now = waiting(&policy, Operation::Live, now);
    policy.stable_live();
    policy.failed_with_jitter(
        Operation::Live,
        ENDPOINT,
        Failure::Temporary,
        "offline".into(),
        now,
        Duration::ZERO,
    );
    assert_eq!(waiting(&policy, Operation::Live, now) - now, LIVE_DELAY);
}

#[test]
fn permanent_http_errors_stay_stopped_but_do_not_disable_other_channels() {
    for status in [
        StatusCode::BAD_REQUEST,
        StatusCode::UNAUTHORIZED,
        StatusCode::FORBIDDEN,
        StatusCode::NOT_FOUND,
        StatusCode::FOUND,
    ] {
        let mut policy = Policy::default();
        let now = Instant::now();
        policy.failed_with_jitter(
            Operation::Live,
            ENDPOINT,
            Failure::Http {
                status,
                retry_after: None,
            },
            status.to_string(),
            now,
            JITTER,
        );
        policy.stable_live();
        policy.activity_succeeded(now);
        assert!(matches!(
            policy.check(Operation::Live, ENDPOINT, now + Duration::from_secs(86400)),
            Err(Blocked::Stopped(_))
        ));
        assert!(
            policy
                .check(Operation::Live, "another channel", now)
                .is_ok()
        );
    }
}
