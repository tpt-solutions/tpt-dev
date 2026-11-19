//! A view stored by the closure outlives the call that made it. Must not compile.
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
    let mut streaming = Session::new(&mut backend)
        .create()
        .unwrap()
        .open()
        .unwrap()
        .start()
        .unwrap();
    let mut kept: Option<View<'_>> = None;
    let _ = streaming.with_view(|view| kept = Some(view));
}
