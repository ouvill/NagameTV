//! Owned tsreadex service filter. Input is complete, aligned 188-byte TS packets.
//! Recreate on seek: ClearPackets only empties output, not parser state.
#[cxx::bridge(namespace = "viewer")]
mod ffi {
    unsafe extern "C++" {
        include!("filter.hpp");
        type Filter;
        fn make_filter(service: u16) -> Result<UniquePtr<Filter>>;
        fn push<'a>(self: Pin<&'a mut Filter>, packets: &[u8]) -> Result<&'a [u8]>;
    }
}

pub struct Filter(cxx::UniquePtr<ffi::Filter>);

// CServiceFilter owns its vectors/parser and has no thread-affine resources.
// Only exclusive &mut access crosses the bridge; it is never shared concurrently.
unsafe impl Send for Filter {}

impl Filter {
    pub fn new(service: u16) -> Result<Self, cxx::Exception> {
        ffi::make_filter(service).map(Self)
    }
    /// Borrows the native output until the next mutable use of this filter.
    ///
    /// ```compile_fail
    /// let mut filter = tsreadex::Filter::new(1).unwrap();
    /// let output = filter.push(&[]).unwrap();
    /// filter.push(&[]).unwrap();
    /// assert!(output.is_empty());
    /// ```
    pub fn push(&mut self, packets: &[u8]) -> Result<&[u8], cxx::Exception> {
        self.0.pin_mut().push(packets)
    }
}

#[cfg(test)]
mod tests {
    use super::Filter;

    #[test]
    fn borrowed_output_matches_packet_and_batch_feeding() {
        const PACKET_BYTES: usize = 188;
        let input = include_bytes!("../../../../tests/fixtures/recording-caption-change.ts");
        let mut filter = Filter::new(1).unwrap();
        let expected = filter.push(input).unwrap().to_vec();
        assert!(!expected.is_empty());
        assert!(filter.push(&[]).unwrap().is_empty());
        for batch in [1, 256] {
            let mut filter = Filter::new(1).unwrap();
            let mut output = Vec::new();
            for packets in input.chunks(PACKET_BYTES * batch) {
                output.extend_from_slice(filter.push(packets).unwrap());
            }
            assert_eq!(output, expected);
        }
    }

    #[test]
    fn invalid_input_is_rejected_before_exposing_output() {
        const PACKET_BYTES: usize = 188;
        let mut filter = Filter::new(1).unwrap();
        assert!(filter.push(&[0; PACKET_BYTES - 1]).is_err());
        assert!(filter.push(&[0; PACKET_BYTES]).is_err());
        assert!(filter.push(&[]).unwrap().is_empty());
    }
}
