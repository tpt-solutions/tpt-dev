//! One dependency for the tpt-dev crates.
//!
//! Enable a feature group and each crate in it appears as a module. Each
//! module is the crate itself, so its documentation applies unchanged.
//!
//! ```toml
//! [dependencies]
//! tpt-dev = { version = "0.0.0", features = ["verify", "rt"] }
//! ```
//!
//! | Feature | Modules |
//! |---|---|
//! | `verify` | `solver`, `automata`, `test`, `fuzz`, `mock` |
//! | `contract` | `contract` (proc-macro, host only; implies `verify`) |
//! | `rt` | `async_rt`, `trace`, `lock`, `sink`, `replay` |
//! | `ffi` | `ffi`, `codec`, `compose` |
//! | `security` | `pqc`, `tee`, `zk` |
//! | `edge` | `mem`, `paged`, `dispatch`, `sim`, `profiler` |
//! | `lang` | `ast`, `dsl`, `codegen` |
//! | `bench` | `bench` |
//! | `tools` | `headers`, `lint`, `dep`, `doc` (host only, uses `std`) |
//! | `full` | every `no_std` library group, except `contract` and `tools` |
//! | `std` | forwards `std` to every enabled crate that has one |
//!
//! `async` is a keyword, so the async runtime is `async_rt`.
//!
//! The default build enables nothing and is `no_std`.

#![no_std]

#[cfg(feature = "lang")]
pub use tpt_dev_ast as ast;
#[cfg(feature = "rt")]
pub use tpt_dev_async as async_rt;
#[cfg(feature = "verify")]
pub use tpt_dev_automata as automata;
#[cfg(feature = "bench")]
pub use tpt_dev_bench as bench;
#[cfg(feature = "ffi")]
pub use tpt_dev_codec as codec;
#[cfg(feature = "lang")]
pub use tpt_dev_codegen as codegen;
#[cfg(feature = "ffi")]
pub use tpt_dev_compose as compose;
#[cfg(feature = "contract")]
pub use tpt_dev_contract::{ensures, requires};
#[cfg(feature = "tools")]
pub use tpt_dev_dep as dep;
#[cfg(feature = "edge")]
pub use tpt_dev_dispatch as dispatch;
#[cfg(feature = "tools")]
pub use tpt_dev_doc as doc;
#[cfg(feature = "lang")]
pub use tpt_dev_dsl as dsl;
#[cfg(feature = "ffi")]
pub use tpt_dev_ffi as ffi;
#[cfg(feature = "verify")]
pub use tpt_dev_fuzz as fuzz;
#[cfg(feature = "tools")]
pub use tpt_dev_headers as headers;
#[cfg(feature = "tools")]
pub use tpt_dev_lint as lint;
#[cfg(feature = "rt")]
pub use tpt_dev_lock as lock;
#[cfg(feature = "edge")]
pub use tpt_dev_mem as mem;
#[cfg(feature = "verify")]
pub use tpt_dev_mock as mock;
#[cfg(feature = "edge")]
pub use tpt_dev_paged as paged;
#[cfg(feature = "security")]
pub use tpt_dev_pqc as pqc;
#[cfg(feature = "edge")]
pub use tpt_dev_profiler as profiler;
#[cfg(feature = "rt")]
pub use tpt_dev_replay as replay;
#[cfg(feature = "edge")]
pub use tpt_dev_sim as sim;
#[cfg(feature = "rt")]
pub use tpt_dev_sink as sink;
#[cfg(feature = "verify")]
pub use tpt_dev_solver as solver;
#[cfg(feature = "security")]
pub use tpt_dev_tee as tee;
#[cfg(feature = "verify")]
pub use tpt_dev_test as test;
#[cfg(feature = "rt")]
pub use tpt_dev_trace as trace;
#[cfg(feature = "security")]
pub use tpt_dev_zk as zk;

/// The types most programs reach for first. Import with `use tpt_dev::prelude::*;`.
pub mod prelude {
    #[cfg(feature = "rt")]
    pub use crate::async_rt::Executor;
    #[cfg(feature = "verify")]
    pub use crate::automata::Machine;
    #[cfg(feature = "verify")]
    pub use crate::solver::{Solver, Status};
    #[cfg(feature = "rt")]
    pub use crate::trace::{Collector, Context, SpanId, TraceId};
}
