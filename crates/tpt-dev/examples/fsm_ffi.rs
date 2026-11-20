//! One state machine used three ways: to check it has no deadlock, to drive a
//! C-style library through typestate, and to generate the header for it.
//!
//! The library here is a Rust counter exported with `extern "C"`, so no C
//! compiler is needed. `tpt-dev-headers` reads that source and emits C.
//!
//! Run with: `cargo run -p tpt-dev --example fsm_ffi --features verify,ffi,tools`

use tpt_dev::automata::Machine;
use tpt_dev::ffi::{Backend, Session};
use tpt_dev::headers::{Options, generate};

/// The lifecycle the typestate session enforces at compile time, written out
/// as a machine so the analysis can check it has no deadlock.
fn lifecycle() -> bool {
    let mut m = Machine::new();
    let closed = m.state("closed");
    let open = m.state("open");
    let streaming = m.state("streaming");
    m.mark_final(closed);
    m.transition(closed, open, "open", None).unwrap();
    m.transition(open, streaming, "start", None).unwrap();
    m.transition(streaming, open, "stop", None).unwrap();
    m.transition(open, closed, "close", None).unwrap();
    m.analyze(closed).unwrap().is_deadlock_free()
}

/// A backend that records calls. A real one would call the C library.
#[derive(Default)]
struct Counter {
    calls: Vec<&'static str>,
}

impl Backend for Counter {
    type Handle = u32;

    fn create(&mut self) -> Result<u32, tpt_dev::ffi::Code> {
        self.calls.push("create");
        Ok(1)
    }
    fn open(&mut self, _h: u32) -> Result<(), tpt_dev::ffi::Code> {
        self.calls.push("open");
        Ok(())
    }
    fn start(&mut self, _h: u32) -> Result<(), tpt_dev::ffi::Code> {
        self.calls.push("start");
        Ok(())
    }
    fn stop(&mut self, _h: u32) -> Result<(), tpt_dev::ffi::Code> {
        self.calls.push("stop");
        Ok(())
    }
    fn close(&mut self, _h: u32) -> Result<(), tpt_dev::ffi::Code> {
        self.calls.push("close");
        Ok(())
    }
    fn destroy(&mut self, _h: u32) {
        self.calls.push("destroy");
    }
}

fn drive(backend: &mut Counter) -> Result<(), i32> {
    let session = Session::new(backend).create().map_err(|f| f.code.0)?;
    let open = session.open().map_err(|f| f.code.0)?;
    let streaming = open.start().map_err(|f| f.code.0)?;
    let open = streaming.stop().map_err(|f| f.code.0)?;
    let closed = open.close().map_err(|f| f.code.0)?;
    drop(closed);
    Ok(())
}

/// The library's exported surface, as Rust. The header is generated from it.
const LIBRARY: &str = r#"
    /// Creates a counter. Returns null on failure.
    #[unsafe(no_mangle)]
    pub extern "C" fn counter_create() -> i32 { 1 }

    /// Starts counting.
    #[unsafe(no_mangle)]
    pub extern "C" fn counter_start(handle: i32) -> i32 { handle }
"#;

fn main() {
    println!("lifecycle has no deadlock: {}", lifecycle());

    let mut backend = Counter::default();
    match drive(&mut backend) {
        Ok(()) => println!("calls: {}", backend.calls.join(" -> ")),
        Err(code) => println!("failed with code {code}"),
    }

    let header = generate(
        LIBRARY,
        &Options {
            guard: "COUNTER_H".into(),
        },
    )
    .expect("library source is supported");
    println!("{header}");
}
