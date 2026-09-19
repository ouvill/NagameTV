//! A completed response survives local import failures and process restarts.
//! Publication consumes validated input; a partial download cannot create coverage.
use super::{Error, Interval, store::Store};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const METADATA_BYTES: u64 = 16 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Receipt {
    pub id: String,
    pub channel: u16,
    pub range: Interval,
    pub fetched: i64,
    pub generation: i64,
}
#[derive(Serialize, Deserialize)]
enum Metadata {
    Receiving(Receipt),
    Downloaded(Receipt),
}
pub(super) enum Recovery {
    None,
    Partial,
    Downloaded(Downloaded),
}
pub(super) struct Receiving {
    directory: PathBuf,
    receipt: Receipt,
    file: File,
}
pub(super) struct Downloaded {
    directory: PathBuf,
    pub receipt: Receipt,
}
pub(super) struct Validated(Downloaded);

fn save(directory: &Path, metadata: &Metadata) -> Result<(), Error> {
    let mut file = tempfile::NamedTempFile::new_in(directory)?;
    serde_json::to_writer(&mut file, metadata).map_err(|e| Error::Format(e.to_string()))?;
    file.as_file_mut().sync_all()?;
    file.persist(directory.join("response.meta"))
        .map_err(|e| e.error)?;
    // Linux/Unix directory sync persists the rename as well as the file data.
    #[cfg(unix)]
    File::open(directory)?.sync_all()?;
    Ok(())
}
pub(super) fn recover(directory: &Path) -> Result<Recovery, Error> {
    let file = match File::open(directory.join("response.meta")) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // Preparation may have stopped between creating the file and its
            // Receiving record; neither artifact proves a complete response.
            if directory.join("response.part").exists() {
                discard(directory)?;
            }
            return Ok(Recovery::None);
        }
        Err(error) => return Err(error.into()),
    };
    if file.metadata()?.len() > METADATA_BYTES {
        return Err(Error::Format("response metadata is too large".into()));
    }
    let metadata: Metadata = serde_json::from_reader(file.take(METADATA_BYTES))
        .map_err(|e| Error::Format(e.to_string()))?;
    match metadata {
        Metadata::Receiving(receipt) => {
            if directory.join("response.json").is_file() {
                // Only EOF + file sync can rename .part to .json. A failed
                // metadata write must not discard that already complete body.
                save(directory, &Metadata::Downloaded(receipt.clone()))?;
                Ok(Recovery::Downloaded(Downloaded {
                    directory: directory.to_owned(),
                    receipt,
                }))
            } else {
                discard(directory)?;
                Ok(Recovery::Partial)
            }
        }
        Metadata::Downloaded(receipt) => Ok(Recovery::Downloaded(Downloaded {
            directory: directory.to_owned(),
            receipt,
        })),
    }
}
pub(super) fn discard(directory: &Path) -> Result<(), Error> {
    for name in ["response.part", "response.json", "response.meta"] {
        match fs::remove_file(directory.join(name)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
impl Receiving {
    pub fn target(&self) -> (u16, Interval) {
        (self.receipt.channel, self.receipt.range)
    }
    pub fn prepare(directory: &Path, receipt: Receipt) -> Result<Self, Error> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join("response.part"))?;
        save(directory, &Metadata::Receiving(receipt.clone()))?;
        Ok(Self {
            directory: directory.to_owned(),
            receipt,
            file,
        })
    }
    pub fn write(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.file.write_all(bytes)?;
        Ok(())
    }
    pub fn finish(mut self) -> Result<Downloaded, Error> {
        self.file.flush()?;
        self.file.sync_all()?;
        fs::rename(
            self.directory.join("response.part"),
            self.directory.join("response.json"),
        )?;
        save(&self.directory, &Metadata::Downloaded(self.receipt.clone()))?;
        Ok(Downloaded {
            directory: self.directory,
            receipt: self.receipt,
        })
    }
}
impl Downloaded {
    #[cfg(test)]
    pub fn validate(self) -> Result<Validated, (Self, Error)> {
        self.validate_with(|| false)
    }
    pub fn validate_with(
        self,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<Validated, (Self, Error)> {
        let result = (|| {
            let file = File::open(self.directory.join("response.json"))?;
            let reader = CancellableReader {
                file,
                cancelled: &mut cancelled,
            };
            match crate::archive::visit(reader, |_| Ok::<_, std::convert::Infallible>(())) {
                Ok(_) => Ok(()),
                Err(crate::archive::VisitError::Input(crate::archive::Error::Io(error))) => {
                    Err(Error::Io(error))
                }
                Err(crate::archive::VisitError::Input(error)) => Err(Error::Archive(error)),
                Err(crate::archive::VisitError::Sink(never)) => match never {},
            }
        })();
        let result = if cancelled() {
            Err(Error::Cancelled)
        } else {
            result
        };
        match result {
            Ok(()) => Ok(Validated(self)),
            Err(error) => Err((self, error)),
        }
    }
    pub fn discard(self) -> Result<(), Error> {
        discard(&self.directory)
    }
}
struct CancellableReader<'a, F> {
    file: File,
    cancelled: &'a mut F,
}
impl<F: FnMut() -> bool> Read for CancellableReader<'_, F> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if (self.cancelled)() {
            return Err(std::io::Error::other("comment import cancelled"));
        }
        self.file.read(buffer)
    }
}
impl Validated {
    pub fn import(
        &self,
        store: &mut Store,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<(), Error> {
        let receipt = &self.0.receipt;
        if store.imported(&receipt.id)? {
            return Ok(());
        }
        if !store.can_publish(receipt)? {
            return Ok(());
        }
        let id = store.begin_import(receipt)?;
        let mut batch = Vec::with_capacity(super::store::IMPORT_BATCH);
        let file = File::open(self.0.directory.join("response.json"))?;
        let reader = CancellableReader {
            file,
            cancelled: &mut cancelled,
        };
        let result = crate::archive::visit(reader, |comment| {
            batch.push(comment);
            if batch.len() == super::store::IMPORT_BATCH {
                store.import_batch(id, receipt.range, &mut batch)?;
            }
            Ok(())
        });
        if cancelled() {
            return Err(Error::Cancelled);
        }
        match result {
            Ok(_) => {}
            Err(crate::archive::VisitError::Input(error)) => return Err(error.into()),
            Err(crate::archive::VisitError::Sink(error)) => return Err(error),
        }
        store.import_batch(id, receipt.range, &mut batch)?;
        if cancelled() {
            return Err(Error::Cancelled);
        }
        store.publish(id, receipt)
    }
    pub fn finish(self) -> Result<(), Error> {
        self.0.discard()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_body_survives_failure_between_rename_and_metadata_commit() {
        let dir = tempfile::tempdir().unwrap();
        let receipt = Receipt {
            id: "rename".into(),
            channel: 1,
            range: Interval::new(100, 200).unwrap(),
            fetched: 100_000,
            generation: 0,
        };
        let mut rx = Receiving::prepare(dir.path(), receipt).unwrap();
        rx.write(br#"{"packet":[]}"#).unwrap();
        rx.file.sync_all().unwrap();
        fs::rename(
            dir.path().join("response.part"),
            dir.path().join("response.json"),
        )
        .unwrap();
        drop(rx);
        let Recovery::Downloaded(file) = recover(dir.path()).unwrap() else {
            panic!("already received body must be reused")
        };
        assert!(file.validate().is_ok());
    }
    #[test]
    fn invalid_trailer_never_publishes_and_complete_download_recovers_locally() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let receipt = Receipt {
            id: "one".into(),
            channel: 1,
            range: Interval::new(100, 200).unwrap(),
            fetched: 100_000,
            generation: 0,
        };
        let mut rx = Receiving::prepare(dir.path(), receipt.clone()).unwrap();
        rx.write(br#"{"packet":[{"chat":{"date":150,"content":"ok"}}]}"#)
            .unwrap();
        drop(rx.finish().unwrap());
        let Recovery::Downloaded(file) = recover(dir.path()).unwrap() else {
            panic!("complete response must survive restart")
        };
        let valid = file.validate().map_err(|(_, e)| e).unwrap();
        assert!(matches!(
            valid.import(&mut store, || true),
            Err(Error::Cancelled)
        ));
        assert!(store.covered(1, receipt.range, 100_000).unwrap().is_empty());
        valid.import(&mut store, || false).unwrap();
        valid.finish().unwrap();
        assert_eq!(
            store.covered(1, receipt.range, 100_000).unwrap(),
            vec![receipt.range]
        );
        let mut rx = Receiving::prepare(
            dir.path(),
            Receipt {
                id: "two".into(),
                ..receipt.clone()
            },
        )
        .unwrap();
        rx.write(br#"{"packet":[],"error":"failed after packet"}"#)
            .unwrap();
        let file = rx.finish().unwrap();
        assert!(file.validate().is_err());
        assert_eq!(
            store.covered(1, receipt.range, 100_000).unwrap(),
            vec![receipt.range]
        );
    }
}
