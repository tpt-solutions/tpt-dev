//! Helpers for `build.rs`. They need the `std` feature.

use std::fs;
use std::io;
use std::path::Path;

/// Writes `contents` to `path` only if the file does not already hold them.
///
/// Returns `true` if the file was written. Leaving an unchanged file alone
/// keeps its timestamp, so dependent crates are not rebuilt for nothing.
///
/// ```no_run
/// use tpt_dev_codegen::build::write_if_changed;
///
/// let out = std::path::Path::new("generated.rs");
/// write_if_changed(out, "pub const X: u8 = 1;\n").unwrap();
/// ```
pub fn write_if_changed(path: &Path, contents: &str) -> io::Result<bool> {
    match fs::read_to_string(path) {
        Ok(existing) if existing == contents => Ok(false),
        Ok(_) | Err(_) => {
            fs::write(path, contents)?;
            Ok(true)
        }
    }
}

/// Tells Cargo to rerun the build script when `path` changes.
///
/// Call it once for each input the generated code depends on. Prints to
/// standard output, which Cargo reads.
pub fn rerun_if_changed(path: &Path) {
    std::println!("cargo:rerun-if-changed={}", path.display());
}

/// Tells Cargo to rerun the build script when environment variable `name` changes.
pub fn rerun_if_env_changed(name: &str) {
    std::println!("cargo:rerun-if-env-changed={name}");
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn unchanged_contents_are_not_rewritten() {
        let dir = std::env::temp_dir().join("tpt-dev-codegen-test");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("out.rs");
        assert!(write_if_changed(&path, "a\n").unwrap());
        assert!(!write_if_changed(&path, "a\n").unwrap());
        assert!(write_if_changed(&path, "b\n").unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), "b\n");
    }
}
