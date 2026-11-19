//! Starting a session that is not open must not compile.
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
    let closed = Session::new(&mut backend).create().unwrap();
    let _ = closed.start();
}
