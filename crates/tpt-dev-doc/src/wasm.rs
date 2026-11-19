//! Compiling runnable examples to Wasm, and the browser runner that runs them.
//!
//! The compile step runs `rustc` and needs `std`. It is a build-time step on the
//! maintainer's machine, and only for trusted documentation. The runner,
//! [`RUNNER_JS`], runs in the reader's browser, in a Web Worker with a time limit.
//!
//! The Wasm module is built with a 16 MiB memory cap. It imports nothing, so it
//! has no access to the page, the network or the file system.
//!
//! Not handled: examples that define `fn main`, examples with inner attributes
//! (`#![...]`), and examples that need a crate other than `std` and `core`.

use alloc::format;
use alloc::string::String;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;

use crate::extract::{Example, Mode};

/// Memory cap for the Wasm module, in bytes (16 MiB).
const MAX_MEMORY: u32 = 16 * 1024 * 1024;

/// The Rust source to compile for `example`, or `None` when the example is not
/// run as Wasm. Only [`Mode::Run`] examples are built. The body becomes an
/// exported `run` function, so a panic in the example traps.
pub fn wasm_source(example: &Example) -> Option<String> {
    if example.mode != Mode::Run || example.source.contains("fn main") {
        return None;
    }
    let mut out =
        String::from("#![allow(unused)]\n#[unsafe(no_mangle)]\npub extern \"C\" fn run() {\n");
    out.push_str(&example.source);
    out.push_str("}\n");
    Some(out)
}

/// Compiles `example` to `out_file` as a Wasm module, using `rustc`.
///
/// The source is written to `work_dir/example.rs`. The `RUSTC` environment
/// variable selects the compiler, as in cargo. On failure, the error holds
/// rustc's stderr.
pub fn compile(example: &Example, work_dir: &Path, out_file: &Path) -> io::Result<()> {
    let source = wasm_source(example).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "only runnable examples compile to Wasm",
        )
    })?;
    fs::create_dir_all(work_dir)?;
    let src = work_dir.join("example.rs");
    fs::write(&src, source)?;
    let rustc: OsString = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let output = Command::new(rustc)
        .args([
            "--edition",
            "2021",
            "--target",
            "wasm32-unknown-unknown",
            "--crate-type",
            "cdylib",
            "-C",
            "panic=abort",
            "-C",
            "opt-level=s",
        ])
        .arg("-C")
        .arg(format!("link-arg=--max-memory={MAX_MEMORY}"))
        .arg("-o")
        .arg(out_file)
        .arg(&src)
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ))
    }
}

/// Browser code that runs one compiled example. It is a static ES module with
/// no external reference. Load it in a page, then call
/// `runExample(wasmUrl, timeoutMs)`. It returns `{status, detail}`, where
/// status is `ok`, `failed` or `timeout`.
///
/// The `detail` text must be shown with `textContent`, never `innerHTML`.
pub const RUNNER_JS: &str = r#"// tpt-dev-doc runner: runs one compiled example in a Web Worker with a time limit.
const WORKER_SOURCE = `
self.onmessage = async (event) => {
  try {
    const { instance } = await WebAssembly.instantiate(event.data, {});
    instance.exports.run();
    self.postMessage({ status: "ok", detail: "" });
  } catch (error) {
    self.postMessage({ status: "failed", detail: String(error && error.message) });
  }
};`;

export async function runExample(wasmUrl, timeoutMs = 5000) {
  let bytes;
  try {
    bytes = await (await fetch(wasmUrl)).arrayBuffer();
  } catch (error) {
    return { status: "failed", detail: "could not load the example" };
  }
  const url = URL.createObjectURL(new Blob([WORKER_SOURCE], { type: "text/javascript" }));
  const worker = new Worker(url);
  try {
    return await new Promise((resolve) => {
      const timer = setTimeout(() => resolve({ status: "timeout", detail: "" }), timeoutMs);
      worker.onmessage = (event) => {
        clearTimeout(timer);
        resolve(event.data);
      };
      worker.onerror = () => {
        clearTimeout(timer);
        resolve({ status: "failed", detail: "worker error" });
      };
      worker.postMessage(bytes);
    });
  } finally {
    worker.terminate();
    URL.revokeObjectURL(url);
  }
}
"#;

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::extract::extract;
    use std::eprintln;

    fn first(md: &str) -> Example {
        extract(md).into_iter().next().unwrap()
    }

    #[test]
    fn only_run_examples_become_wasm_source() {
        let run = first("```rust\nassert_eq!(1, 1);\n```\n");
        let src = wasm_source(&run).unwrap();
        assert!(src.contains("pub extern \"C\" fn run() {"));
        assert!(src.contains("assert_eq!(1, 1);"));

        for md in [
            "```rust,ignore\nx\n```\n",
            "```rust,no_run\nx\n```\n",
            "```rust,compile_fail\nx\n```\n",
            "```rust,should_panic\nx\n```\n",
            "```toml\nx = 1\n```\n",
        ] {
            assert_eq!(wasm_source(&first(md)), None, "{md}");
        }
    }

    #[test]
    fn examples_with_a_main_are_not_built() {
        assert_eq!(wasm_source(&first("```rust\nfn main() {}\n```\n")), None);
    }

    #[test]
    fn the_runner_has_no_network_or_html_sinks() {
        for banned in [
            "innerHTML",
            "outerHTML",
            "eval(",
            "importScripts",
            "<script",
        ] {
            assert!(!RUNNER_JS.contains(banned), "{banned}");
        }
        assert!(RUNNER_JS.contains("terminate()"));
        assert!(RUNNER_JS.contains("timeoutMs"));
    }

    #[test]
    fn compiles_a_runnable_example_to_wasm() {
        let dir = std::env::temp_dir().join(format!("tpt-dev-doc-wasm-{}", std::process::id()));
        let out = dir.join("example.wasm");
        let example = first("```rust\nlet x: u32 = 2 + 2;\nassert_eq!(x, 4);\n```\n");

        let result = compile(&example, &dir, &out);
        let bytes = fs::read(&out);
        let _ = fs::remove_dir_all(&dir);

        result.unwrap();
        assert_eq!(&bytes.unwrap()[..4], b"\0asm");
    }

    #[test]
    fn refuses_to_compile_a_non_runnable_example() {
        let dir = std::env::temp_dir().join("tpt-dev-doc-wasm-refuse");
        let err = compile(&first("```text\nx\n```\n"), &dir, &dir.join("x.wasm")).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    /// Loads a module, checks that it imports nothing, and calls `run`.
    const NODE_CHECK: &str = r#"import { readFileSync } from "node:fs";
const module = await WebAssembly.compile(readFileSync(process.argv[2]));
if (WebAssembly.Module.imports(module).length !== 0) {
  console.error("module has imports");
  process.exit(3);
}
const instance = await WebAssembly.instantiate(module, {});
instance.exports.run();
console.log("ok");
"#;

    fn node_available() -> bool {
        Command::new("node")
            .arg("--version")
            .output()
            .is_ok_and(|out| out.status.success())
    }

    /// Compiles `md` and runs it under Node. Returns whether Node exited with
    /// success, and what it printed.
    fn run_under_node(name: &str, md: &str) -> (bool, String) {
        let dir =
            std::env::temp_dir().join(format!("tpt-dev-doc-node-{}-{name}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let wasm = dir.join("example.wasm");
        let script = dir.join("check.mjs");
        let compiled = compile(&first(md), &dir, &wasm);
        fs::write(&script, NODE_CHECK).unwrap();
        let output = Command::new("node").arg(&script).arg(&wasm).output();
        let _ = fs::remove_dir_all(&dir);
        compiled.unwrap();
        let output = output.unwrap();
        (
            output.status.success(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
        )
    }

    #[test]
    fn examples_run_under_node_when_node_is_present() {
        if !node_available() {
            eprintln!("SKIPPED: node is not on the PATH; the Node check did not run");
            return;
        }
        let (passed, stdout) = run_under_node("pass", "```rust\nassert_eq!(2 + 2, 4);\n```\n");
        assert!(passed, "a passing example must return");
        assert_eq!(stdout.trim(), "ok");

        let (failed, _) = run_under_node("fail", "```rust\nassert_eq!(2 + 2, 5);\n```\n");
        assert!(!failed, "a failing assert must trap");
    }
}
