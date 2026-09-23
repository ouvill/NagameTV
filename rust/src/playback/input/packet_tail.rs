//! HTTP reads need not end on a TS boundary. Only the split packet is copied.
use super::TS_PACKET_SIZE;

pub(super) struct PacketTail {
    bytes: [u8; TS_PACKET_SIZE],
    len: usize,
}
impl Default for PacketTail {
    fn default() -> Self {
        Self {
            bytes: [0; TS_PACKET_SIZE],
            len: 0,
        }
    }
}
impl PacketTail {
    pub fn push<E>(
        &mut self,
        mut data: &[u8],
        mut consume: impl FnMut(&[u8]) -> Result<(), E>,
    ) -> Result<(), E> {
        if self.len != 0 {
            let count = data.len().min(TS_PACKET_SIZE - self.len);
            self.bytes[self.len..self.len + count].copy_from_slice(&data[..count]);
            self.len += count;
            data = &data[count..];
            if self.len < TS_PACKET_SIZE {
                return Ok(());
            }
            self.len = 0;
            consume(&self.bytes)?;
        }
        let complete = data.len() / TS_PACKET_SIZE * TS_PACKET_SIZE;
        if complete != 0 {
            consume(&data[..complete])?;
        }
        let tail = &data[complete..];
        self.bytes[..tail.len()].copy_from_slice(tail);
        self.len = tail.len();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn aligned_input_is_borrowed_and_a_split_packet_is_assembled_once() {
        let data = [0x47; TS_PACKET_SIZE * 3];
        let mut tail = PacketTail::default();
        tail.push(&data, |bytes| {
            assert_eq!(bytes.as_ptr(), data.as_ptr());
            assert_eq!(bytes.len(), data.len());
            Ok::<_, ()>(())
        })
        .unwrap();
        tail.push::<()>(&data[..1], |_| panic!("incomplete packet"))
            .unwrap();
        let mut calls = 0;
        tail.push(&data[1..], |bytes| {
            calls += 1;
            if calls == 1 {
                assert_eq!(bytes, &data[..TS_PACKET_SIZE]);
            } else {
                assert_eq!(bytes.as_ptr(), data[TS_PACKET_SIZE..].as_ptr());
            }
            Ok::<_, ()>(())
        })
        .unwrap();
        assert_eq!(calls, 2);
        assert_eq!(tail.len, 0);
    }

    proptest! {
        #[test]
        fn arbitrary_http_boundaries_preserve_complete_packets(
            data in prop::collection::vec(any::<u8>(), 0..10_000),
            sizes in prop::collection::vec(1usize..2048, 1..40),
        ) {
            let mut tail = PacketTail::default();
            let mut output = Vec::new();
            let mut position = 0;
            for count in sizes.iter().cycle() {
                if position == data.len() { break; }
                let end = (position + count).min(data.len());
                tail.push(&data[position..end], |bytes| {
                    assert!(bytes.len().is_multiple_of(TS_PACKET_SIZE));
                    output.extend_from_slice(bytes);
                    Ok::<_, ()>(())
                }).unwrap();
                prop_assert!(tail.len < TS_PACKET_SIZE);
                position = end;
            }
            let complete = data.len() / TS_PACKET_SIZE * TS_PACKET_SIZE;
            prop_assert_eq!(output, &data[..complete]);
            prop_assert_eq!(&tail.bytes[..tail.len], &data[complete..]);
        }
    }
}
