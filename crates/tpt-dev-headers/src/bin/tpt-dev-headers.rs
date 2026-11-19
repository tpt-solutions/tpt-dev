//! Command-line front end: `tpt-dev-headers <SOURCE.rs> <GUARD> <OUTPUT.h>`.
//!
//! Exit codes: `0` the header was written or already up to date, `1` the
//! header could not be generated or written, `2` the arguments were wrong.

use std::path::Path;
use std::process::ExitCode;

use tpt_dev_headers::build::write_header;

const USAGE: &str = "usage: tpt-dev-headers <SOURCE.rs> <GUARD> <OUTPUT.h>

Writes a C header for the C-ABI items in SOURCE.rs. GUARD is the include
guard macro name. The file is rewritten only when its contents change.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let [source, guard, out] = args.as_slice() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    if guard.is_empty() {
        eprintln!("tpt-dev-headers: the include guard must not be empty");
        return ExitCode::from(2);
    }
    match write_header(Path::new(source), Path::new(out), guard) {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("tpt-dev-headers: {e}");
            ExitCode::from(1)
        }
    }
}
