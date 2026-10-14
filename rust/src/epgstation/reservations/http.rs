use super::{Error, Operation, Snapshot, data};
use crate::epgstation::{MAX_RESPONSE_BYTES, client::Connection};
use reqwest::Method;
use serde::Deserialize;
use serde::de::DeserializeOwned;

fn network(error: reqwest::Error) -> Error {
    Error::Http(error.without_url())
}
async fn body(request: reqwest::RequestBuilder, expected: u16) -> Result<Vec<u8>, Error> {
    let mut response = request
        .send()
        .await
        .map_err(network)?
        .error_for_status()
        .map_err(network)?;
    if response.status().as_u16() != expected {
        return Err(Error::Status(response.status().as_u16()));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(network)? {
        if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(Error::Limit);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, Error> {
    // Provider error text and malformed field values can contain credentials.
    crate::json::from_slice(bytes).map_err(|error| Error::Json(super::JsonError(error)))
}
async fn get<T: DeserializeOwned>(connection: &Connection, path: &str) -> Result<T, Error> {
    decode(&body(connection.api_request(Method::GET, path), 200).await?)
}
#[derive(Deserialize)]
struct ReservesPage {
    reserves: Vec<data::Reserve>,
    total: usize,
}
#[derive(Deserialize)]
struct RecordingsPage {
    records: Vec<data::Recording>,
    total: usize,
}
struct Page {
    reserves: Vec<data::Reserve>,
    records: Vec<data::Recording>,
    total: usize,
}
async fn pages(connection: &Connection, recording: bool) -> Result<Page, Error> {
    let mut result = Page {
        reserves: Vec::new(),
        records: Vec::new(),
        total: 0,
    };
    let mut expected = None;
    let mut offset = 0;
    loop {
        let endpoint = if recording { "recording" } else { "reserves" };
        let kind = if recording { "" } else { "type=all&" };
        let path = format!("{endpoint}?{kind}isHalfWidth=false&offset={offset}&limit=100");
        let page = if recording {
            let page: RecordingsPage = get(connection, &path).await?;
            Page {
                records: page.records,
                reserves: Vec::new(),
                total: page.total,
            }
        } else {
            let page: ReservesPage = get(connection, &path).await?;
            Page {
                reserves: page.reserves,
                records: Vec::new(),
                total: page.total,
            }
        };
        let count = if recording {
            page.records.len()
        } else {
            page.reserves.len()
        };
        if page.total > data::MAX_ITEMS {
            return Err(Error::Limit);
        }
        if expected.is_some_and(|total| total != page.total)
            || count > 100
            || offset + count > page.total
            || (count == 0 && offset < page.total)
        {
            return Err(Error::Invalid);
        }
        expected = Some(page.total);
        result.total = page.total;
        result.reserves.extend(page.reserves);
        result.records.extend(page.records);
        offset += count;
        if offset == result.total {
            return Ok(result);
        }
    }
}
pub(super) async fn snapshot(connection: &Connection) -> Result<Snapshot, Error> {
    let reserves = pages(connection, false).await?;
    let recording = pages(connection, true).await?;
    Snapshot::from_rows(reserves.reserves, recording.records)
}
pub(super) async fn execute(
    connection: Connection,
    operation: Operation,
    now: u64,
) -> Result<Snapshot, Error> {
    let started = std::time::Instant::now();
    let current =
        || now.saturating_add(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX));
    match operation {
        Operation::Refresh => {}
        Operation::Add(program) => {
            let schedule: data::Schedule = get(
                &connection,
                &format!("schedules/detail/{}?isHalfWidth=false", program.key.id),
            )
            .await?;
            if !schedule.matches(program) || !program.ongoing(current()) {
                return Err(Error::ProgramChanged);
            }
            // Recheck a complete snapshot before a non-idempotent write; never replay it on failure.
            if snapshot(&connection).await?.status(program).is_some() {
                return Err(Error::AlreadyReserved);
            }
            if !program.ongoing(current()) {
                return Err(Error::ProgramChanged);
            }
            let payload = serde_json::json!({"programId": program.key.id, "allowEndLack": false});
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Added {
                reserve_id: u64,
            }
            let added: Added = decode(
                &body(
                    connection
                        .api_request(Method::POST, "reserves")
                        .header(reqwest::header::CONTENT_TYPE, "application/json")
                        .body(payload.to_string()),
                    201,
                )
                .await?,
            )?;
            if added.reserve_id > (1 << 53) - 1 {
                return Err(Error::Invalid);
            }
        }
        Operation::Cancel { program, id } => {
            let reserve: data::Reserve =
                get(&connection, &format!("reserves/{id}?isHalfWidth=false")).await?;
            if reserve.id != id || !reserve.cancellable(program, current()) {
                return Err(Error::NotCancellable);
            }
            if pages(&connection, true)
                .await?
                .records
                .iter()
                .any(|row| row.matches(program))
            {
                return Err(Error::NotCancellable);
            }
            if !reserve.cancellable(program, current()) {
                return Err(Error::NotCancellable);
            }
            body(
                connection.api_request(Method::DELETE, &format!("reserves/{id}")),
                200,
            )
            .await?;
        }
    }
    snapshot(&connection)
        .await
        .map_err(|error| match operation {
            Operation::Refresh => error,
            Operation::Add(_) | Operation::Cancel { .. } => {
                Error::RefreshAfterWrite(Box::new(error))
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_diagnostics_keep_the_field_and_cause_without_exposing_response_values() {
        let error = decode::<ReservesPage>(br#"{"reserves":[],"total":"private-token"}"#)
            .err()
            .expect("invalid count");
        assert!(error.to_string().contains("total"));
        assert!(!format!("{error:?}").contains("private-token"));
        let mut cause: Option<&dyn std::error::Error> = Some(&error);
        while let Some(error) = cause {
            assert!(!error.to_string().contains("private-token"));
            cause = error.source();
        }
        let Error::Json(error) = error else {
            panic!("expected JSON error");
        };
        let crate::json::Error::Decode { path, source } = &error.0 else {
            panic!("original field cause was lost");
        };
        assert_eq!(path.to_string(), "total");
        assert!(source.is_data());
        assert!(matches!(decode::<Vec<u64>>(b"[] {}"), Err(Error::Json(_))));
    }
}
