//! Owned tsreadex service filter. Input is complete, aligned 188-byte TS packets.
//! Recreate on seek: ClearPackets only empties output, not parser state.
#[cxx::bridge(namespace = "viewer")]
mod ffi {
    unsafe extern "C++" {
        include!("filter.hpp");
        type Filter;
        fn make_filter(service: u16) -> Result<UniquePtr<Filter>>;
        fn push(self: Pin<&mut Filter>, packets: &[u8]) -> Result<Vec<u8>>;
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
    pub fn push(&mut self, packets: &[u8]) -> Result<Vec<u8>, cxx::Exception> {
        self.0.pin_mut().push(packets)
    }
}
