use crate::Error;
use flate2::{Decompress, FlushDecompress, Status};

/// Decode one complete RFC 1950 stream, with bounded output allocation.
pub(super) fn decode(bytes: &[u8], original_size: usize) -> Result<Vec<u8>, Error> {
    const CHUNK_BYTES: usize = 32 * 1024;
    let mut decoder = Decompress::new(true);
    let mut output = Vec::new();
    let mut chunk = [0; CHUNK_BYTES];
    loop {
        let before_in = decoder.total_in();
        let before_out = decoder.total_out();
        let capacity = CHUNK_BYTES.min((original_size - output.len()).saturating_add(1));
        let status = decoder
            .decompress(
                &bytes[before_in as usize..],
                &mut chunk[..capacity],
                FlushDecompress::None,
            )
            .map_err(|error| {
                Error::Decompression(std::io::Error::new(std::io::ErrorKind::InvalidData, error))
            })?;
        let written = (decoder.total_out() - before_out) as usize;
        if written > original_size - output.len() {
            return Err(Error::Invalid("decompressed module size"));
        }
        output.extend_from_slice(&chunk[..written]);
        if status == Status::StreamEnd {
            break;
        }
        if decoder.total_in() == before_in && written == 0 {
            return Err(Error::Invalid("truncated compressed module"));
        }
    }
    if output.len() != original_size {
        return Err(Error::Invalid("decompressed module size"));
    }
    validate_trailer(&bytes[decoder.total_in() as usize..], &output)?;
    Ok(output)
}

fn validate_trailer(trailer: &[u8], output: &[u8]) -> Result<(), Error> {
    if trailer.is_empty() {
        return Ok(());
    }
    // Some broadcasts append an RFC 1952 CRC32/ISIZE trailer after the complete
    // zlib stream. Accept this observed extension only when both fields match.
    // RFC 1950 excludes bytes after ADLER32 from the zlib stream itself.
    let [c0, c1, c2, c3, s0, s1, s2, s3] = *trailer else {
        return Err(Error::Invalid("compressed module trailing bytes"));
    };
    let mut checksum = flate2::Crc::new();
    checksum.update(output);
    if u32::from_le_bytes([c0, c1, c2, c3]) != checksum.sum()
        || u32::from_le_bytes([s0, s1, s2, s3]) != checksum.amount()
    {
        return Err(Error::Invalid("compressed module trailer checksum or size"));
    }
    Ok(())
}
