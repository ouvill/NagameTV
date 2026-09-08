//! Minimal projection of NX-Jikkyo's channel activity response, independent of transport.
use serde::{
    Deserialize, Deserializer,
    de::{self, SeqAccess, Visitor},
};
use std::{borrow::Cow, collections::BTreeMap, fmt};

pub const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_CHANNELS: usize = 256;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Comment activity response exceeds {MAX_RESPONSE_BYTES} bytes")]
    ResponseTooLarge,
    #[error("Invalid comment activity response: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Snapshot(BTreeMap<u16, u64>, BTreeMap<u16, Box<str>>);
impl Snapshot {
    pub fn get(&self, channel: u16) -> Option<u64> {
        self.0.get(&channel).copied()
    }
    pub fn program_title(&self, channel: u16) -> Option<&str> {
        self.1.get(&channel).map(AsRef::as_ref)
    }
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_RESPONSE_BYTES {
            return Err(Error::ResponseTooLarge);
        }
        Ok(serde_json::from_slice(bytes)?)
    }
}

// Deserialize one thread at a time; descriptions and unused program fields are skipped by serde.
// Option<Option<u64>> distinguishes no active thread from an active thread with null force.
#[derive(Default)]
struct ActiveForce(Option<Option<u64>>);
impl<'de> Deserialize<'de> for ActiveForce {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Threads;
        impl<'de> Visitor<'de> for Threads {
            type Value = ActiveForce;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a thread array")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                #[derive(Deserialize)]
                struct Thread<'a> {
                    #[serde(borrow)]
                    status: Cow<'a, str>,
                    jikkyo_force: Option<u64>,
                }
                let mut active = ActiveForce::default();
                while let Some(thread) = seq.next_element::<Thread<'de>>()? {
                    if active.0.is_none() && thread.status == "ACTIVE" {
                        active.0 = Some(thread.jikkyo_force);
                    }
                }
                Ok(active)
            }
        }
        deserializer.deserialize_seq(Threads)
    }
}
impl<'de> Deserialize<'de> for Snapshot {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Channels;
        impl<'de> Visitor<'de> for Channels {
            type Value = Snapshot;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a channel array with at most 256 entries")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Snapshot, A::Error> {
                #[derive(Deserialize)]
                struct Channel<'a> {
                    #[serde(borrow)]
                    id: Cow<'a, str>,
                    threads: ActiveForce,
                    program_present: Option<Program>,
                }
                #[derive(Deserialize)]
                struct Program {
                    title: String,
                }
                let mut snapshot = Snapshot::default();
                let mut count = 0;
                while let Some(channel) = seq.next_element::<Channel<'de>>()? {
                    count += 1;
                    if count > MAX_CHANNELS {
                        return Err(de::Error::custom("too many activity channels (limit: 256)"));
                    }
                    let Some(code) = channel.id.strip_prefix("jk") else {
                        continue;
                    };
                    // Map only canonical IDs produced by the application's channel mapping.
                    if code.starts_with('0') {
                        continue;
                    }
                    let Ok(id) = code.parse::<u16>() else {
                        continue;
                    };
                    if !code.bytes().all(|byte| byte.is_ascii_digit()) {
                        continue;
                    }
                    if let Some(program) = channel.program_present {
                        let title: String = program.title.trim().chars().take(512).collect();
                        if !title.is_empty() {
                            snapshot.1.insert(id, title.into_boxed_str());
                        }
                    }
                    if let Some(force) = channel.threads.0.flatten() {
                        snapshot.0.insert(id, force);
                    }
                }
                Ok(snapshot)
            }
        }
        deserializer.deserialize_seq(Channels)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn titles_are_optional_bounded_and_independent_of_force() -> Result<(), Error> {
        let snapshot = Snapshot::parse(br#"[
            {"id":"jk1","threads":[],"program_present":{"title":"  <b>News</b>  "}},
            {"id":"jk2","threads":[],"program_present":null},
            {"id":"jk4","threads":[]},
            {"id":"jk5","threads":[],"program_present":{"title":" "}}
        ]"#)?;
        assert_eq!(snapshot.program_title(1), Some("<b>News</b>"));
        assert_eq!(snapshot.get(1), None);
        for id in [2, 4, 5] { assert_eq!(snapshot.program_title(id), None); }
        let bytes = serde_json::to_vec(&serde_json::json!([{
            "id":"jk1", "threads":[], "program_present":{"title":"あ".repeat(600)}
        }]))?;
        assert_eq!(Snapshot::parse(&bytes)?.program_title(1).map(|s| s.chars().count()), Some(512));
        Ok(())
    }
    #[test]
    fn first_active_thread_preserves_zero_null_and_large_values() -> Result<(), Error> {
        let snapshot = Snapshot::parse(br#"[
            {"id":"jk1","threads":[{"status":"PAST","jikkyo_force":999},{"status":"ACTIVE","jikkyo_force":0},{"status":"ACTIVE","jikkyo_force":77}]},
            {"id":"jk2","threads":[{"status":"ACTIVE","jikkyo_force":null},{"status":"ACTIVE","jikkyo_force":99}]},
            {"id":"jk3","threads":[{"status":"UPCOMING","jikkyo_force":123}]},
            {"id":"jk101","threads":[{"status":"ACTIVE","jikkyo_force":18446744073709551615}]}
        ]"#)?;
        assert_eq!(snapshot.get(1), Some(0));
        assert_eq!(snapshot.get(2), None);
        assert_eq!(snapshot.get(3), None);
        assert_eq!(snapshot.get(101), Some(u64::MAX));
        Ok(())
    }
    #[test]
    fn limits_and_malformed_data_fail_instead_of_returning_partial_activity() {
        assert!(matches!(
            Snapshot::parse(&vec![b' '; MAX_RESPONSE_BYTES + 1]),
            Err(Error::ResponseTooLarge)
        ));
        let channel = r#"{"id":"jk1","threads":[]}"#;
        assert!(
            Snapshot::parse(format!("[{}]", vec![channel; MAX_CHANNELS].join(",")).as_bytes())
                .is_ok()
        );
        assert!(
            Snapshot::parse(format!("[{}]", vec![channel; MAX_CHANNELS + 1].join(",")).as_bytes())
                .is_err()
        );
        for bytes in [
            b"null".as_slice(),
            b"[] trailing",
            br#"[{"id":"jk1","threads":[{"status":"ACTIVE","jikkyo_force":-1}]}]"#,
        ] {
            assert!(Snapshot::parse(bytes).is_err());
        }
    }
    #[test]
    fn ignores_unmapped_ids_and_unused_payload_without_retaining_threads() -> Result<(), Error> {
        let mut channels = Vec::new();
        for id in ["jk0", "jk01", "jk+1", "other1", "jk65536", "jk65535"] {
            channels.push(format!(r#"{{"id":"{id}","threads":[{{"status":"ACTIVE","jikkyo_force":42}}],"program_present":{{"title":"unused"}}}}"#));
        }
        let snapshot = Snapshot::parse(format!("[{}]", channels.join(",")).as_bytes())?;
        assert_eq!(snapshot.0.len(), 1);
        assert_eq!(snapshot.get(65535), Some(42));
        Ok(())
    }
}
