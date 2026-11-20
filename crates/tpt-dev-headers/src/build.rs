//! Helpers for `build.rs` and the `tpt-dev-headers` command.
//!
//! The crate is a host tool that already needs `std`, so these helpers are
//! always compiled. They follow the same change-detection pattern as
//! `tpt-dev-codegen::build`: a file is rewritten only when its contents differ,
//! so an unchanged header keeps its timestamp and C code that includes it is
//! not rebuilt for nothing.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::{Error, Options, generate};

/// Why [`write_header`] failed.
#[derive(Debug)]
pub enum BuildError {
    /// The Rust source could not be read.
    Read {
        /// The source file.
        path: PathBuf,
        /// The underlying I/O error.
        source: io::Error,
    },
    /// The source could be read, but no header could be generated from it.
    Generate(Error),
    /// The header could not be written.
    Write {
        /// The output file.
        path: PathBuf,
        /// The underlying I/O error.
        source: io::Error,
    },
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(f, "cannot read {}: {source}", path.display())
            }
            Self::Generate(e) => write!(f, "cannot generate header: {e}"),
            Self::Write { path, source } => {
                write!(f, "cannot write {}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for BuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { source, .. } | Self::Write { source, .. } => Some(source),
            Self::Generate(e) => Some(e),
        }
    }
}

/// Generates the header for the Rust file `source` and writes it to `out`.
///
/// The output is written only if it does not already hold the same text.
/// Returns `true` if the file was written.
///
/// Cargo does not watch `source` by itself. A build script should also call
/// [`rerun_if_changed`] for it.
///
/// ```no_run
/// use tpt_dev_headers::build::{rerun_if_changed, write_header};
///
/// let src = std::path::Path::new("src/ffi.rs");
/// rerun_if_changed(src);
/// write_header(src, std::path::Path::new("include/ffi.h"), "FFI_H").unwrap();
/// ```
pub fn write_header(source: &Path, out: &Path, guard: &str) -> Result<bool, BuildError> {
    let text = fs::read_to_string(source).map_err(|e| BuildError::Read {
        path: source.to_path_buf(),
        source: e,
    })?;
    let header = generate(
        &text,
        &Options {
            guard: guard.to_string(),
        },
    )
    .map_err(BuildError::Generate)?;
    write_if_changed(out, &header)
}

/// Writes `contents` to `path` only if the file does not already hold them.
///
/// Returns `true` if the file was written.
fn write_if_changed(path: &Path, contents: &str) -> Result<bool, BuildError> {
    match fs::read_to_string(path) {
        Ok(existing) if existing == contents => Ok(false),
        Ok(_) | Err(_) => {
            fs::write(path, contents).map_err(|e| BuildError::Write {
                path: path.to_path_buf(),
                source: e,
            })?;
            Ok(true)
        }
    }
}

/// Tells Cargo to rerun the build script when `path` changes.
///
/// Prints to standard output, which Cargo reads. Call it only from a build
/// script.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use tpt_dev_headers::build::rerun_if_changed;
///
/// // Inside a build script, this tells Cargo to rebuild when the API changes.
/// rerun_if_changed(Path::new("include/api.rs"));
/// ```
pub fn rerun_if_changed(path: &Path) {
    std::println!("cargo:rerun-if-changed={}", path.display());
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "tpt-dev-headers-build-{}-{name}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn unchanged_header_is_not_rewritten() {
        let dir = scratch("unchanged");
        let src = dir.join("api.rs");
        let out = dir.join("api.h");
        fs::write(
            &src,
            "/// Adds.\n#[unsafe(no_mangle)]\npub extern \"C\" fn add(a: i32) -> i32 { a }\n",
        )
        .unwrap();
        assert!(write_header(&src, &out, "G_H").unwrap());
        assert!(!write_header(&src, &out, "G_H").unwrap());
        assert!(write_header(&src, &out, "OTHER_H").unwrap());
        assert!(
            fs::read_to_string(&out)
                .unwrap()
                .contains("#ifndef OTHER_H")
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_source_is_a_read_error() {
        let dir = scratch("missing");
        let err = write_header(&dir.join("nope.rs"), &dir.join("x.h"), "G").unwrap_err();
        assert!(matches!(err, BuildError::Read { .. }));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unsupported_source_is_a_generate_error() {
        let dir = scratch("generate");
        let src = dir.join("bad.rs");
        fs::write(
            &src,
            "#[unsafe(no_mangle)] pub extern \"C\" fn f(x: [u8; 4]) {}\n",
        )
        .unwrap();
        let err = write_header(&src, &dir.join("bad.h"), "G").unwrap_err();
        assert!(matches!(err, BuildError::Generate(_)));
        assert!(!dir.join("bad.h").exists());
        fs::remove_dir_all(&dir).ok();
    }
}
