//! A library call while a view is live. Must not compile: the session is borrowed by the view's call.
use tpt_dev_ffi::{Backend, Code, Session};

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
    let _ = streaming.with_view(|view| {
        let _ = streaming.with(|_, _| ());
        view.len()
    });
}
