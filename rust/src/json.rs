//! Decode one complete external JSON document while retaining field paths and causes.
use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Decode {
        path: serde_path_to_error::Path,
        #[source]
        source: serde_json::Error,
    },
    #[error("{0}")]
    Trailing(#[from] serde_json::Error),
}

pub fn from_slice<'de, T: Deserialize<'de>>(bytes: &'de [u8]) -> Result<T, Error> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value =
        serde_path_to_error::deserialize(&mut deserializer).map_err(|error| Error::Decode {
            path: error.path().clone(),
            source: error.into_inner(),
        })?;
    // Deserializing a value alone accepts a valid prefix. Preserve from_slice's
    // whole-document validation, including when the decoded collection is empty.
    deserializer.end()?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_whitespace_but_rejects_trailing_values_and_garbage() {
        assert!(from_slice::<Vec<u64>>(b"[] \r\n\t").unwrap().is_empty());
        for bytes in [b"[] []".as_slice(), b"[] null", b"[] garbage"] {
            let error = from_slice::<Vec<u64>>(bytes).unwrap_err();
            assert!(matches!(error, Error::Trailing(_)));
            let source = std::error::Error::source(&error)
                .unwrap()
                .downcast_ref::<serde_json::Error>()
                .unwrap();
            assert!(source.is_syntax());
        }
    }
}
