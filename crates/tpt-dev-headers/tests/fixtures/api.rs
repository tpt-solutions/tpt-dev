//! A small C-ABI surface, as the fixture for the golden test.

/// An opaque handle to a sensor session.
pub struct Session {
    handle: u32,
}

/// A reading from one channel.
#[repr(C)]
pub struct Reading {
    /// Channel number.
    pub channel: u8,
    /// Raw value.
    pub raw: u16,
    /// Scaled value.
    pub scaled: f64,
}

/// Status codes returned by the API.
#[repr(i32)]
pub enum Status {
    /// Everything worked.
    Ok = 0,
    /// The session is closed.
    Closed,
    /// A negative code, for the example.
    Fault = -5,
}

/// The number of channels.
pub const CHANNEL_COUNT: u32 = 8;

/// Samples are produced at this rate, in hertz.
pub const SAMPLE_RATE: u32 = 1000;

/// A callback called for each reading.
pub type OnReading = extern "C" fn(reading: *const Reading, user: *mut u8);

/// Opens a session.
#[unsafe(no_mangle)]
pub extern "C" fn session_open(channel: u8) -> *mut Session {
    let _ = channel;
    core::ptr::null_mut()
}

/// Reads one channel into `out`.
#[unsafe(no_mangle)]
pub extern "C" fn session_read(session: *const Session, channel: u8, out: *mut Reading) -> i32 {
    let _ = (session, channel, out);
    0
}

/// Closes a session.
#[unsafe(no_mangle)]
pub extern "C" fn session_close(session: *mut Session) {
    let _ = session;
}

/// Not part of the C API, so it is left out of the header.
pub fn internal_helper(x: u32) -> u32 {
    x
}
