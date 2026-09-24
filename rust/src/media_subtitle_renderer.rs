#[cxx::bridge(namespace = "media_captions")]
pub(super) mod ffi {
    unsafe extern "C++" {
        include!("media_subtitles/renderer.h");
        type Renderer;
        #[cxx_name = "makeRenderer"]
        fn make_renderer() -> Result<UniquePtr<Renderer>>;
        fn header(self: Pin<&mut Renderer>, bytes: &[u8]);
        fn chunk(self: Pin<&mut Renderer>, bytes: &[u8], start: i64, duration: i64);
        fn script(self: Pin<&mut Renderer>, bytes: &[u8]) -> Result<()>;
        fn reset(self: Pin<&mut Renderer>) -> Result<()>;
        fn render(
            self: Pin<&mut Renderer>,
            milliseconds: i64,
            width: i32,
            height: i32,
            force: bool,
        ) -> Result<Vec<u8>>;
    }
}
