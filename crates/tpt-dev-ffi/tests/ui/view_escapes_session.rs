//! A view returned from a block that owns the session. Must not compile.
use tpt_dev_ffi::{Backend, Code, Session, View};

struct Nothing;
impl Backend for Nothing {
    type Handle = u32;
    fn create(&mut self) -> Result<u32, Code> { Ok(1) }
    fn open(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    fn start(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    fn stop(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    fn close(&mut self, _: u32) -> Result<(), Code> { Ok(()) }
    fn destroy(&mut self, _: u32) {}
}

fn main() {
    let mut backend = Nothing;
    let view: View<'_> = {
        let mut streaming = Session::new(&mut backend)
            .create()
            .unwrap()
            .open()
            .unwrap()
            .start()
            .unwrap();
        streaming.with_view(|view| view).unwrap()
    };
    let _ = view.len();
}
