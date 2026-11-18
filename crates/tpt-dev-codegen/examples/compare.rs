//! The same generated table, written two ways: with raw `format!` strings, as a
//! typical build.rs does, and with the `Writer`. The two outputs are compared.
//!
//! Run with: `cargo run -p tpt-dev-codegen --example compare`

#![allow(clippy::unwrap_used)]

use tpt_dev_codegen::Writer;

const CHANNELS: [(&str, u8); 3] = [("temp", 1), ("pressure", 2), ("humidity", 3)];

/// The raw approach: formatting strings by hand, with the indentation and
/// braces written out in every call.
fn raw() -> String {
    let mut out = String::from("// generated\n\n");
    out.push_str("pub const CHANNEL_COUNT: usize = 3;\n\n");
    out.push_str("pub fn channel_id(name: &str) -> Option<u8> {\n");
    out.push_str("    match name {\n");
    for (name, id) in CHANNELS {
        out.push_str(&format!("        \"{name}\" => Some({id}),\n"));
    }
    out.push_str("        _ => None,\n");
    out.push_str("    }\n");
    out.push_str("}\n");
    out
}

/// The builder approach: the structure of the calls gives the indentation and braces.
fn built() -> String {
    let mut w = Writer::new();
    w.line("// generated");
    w.blank();
    w.constant("pub", "CHANNEL_COUNT", "usize", &CHANNELS.len().to_string())
        .unwrap();
    w.blank();
    w.block("pub fn channel_id(name: &str) -> Option<u8>", |w| {
        w.block("match name", |w| {
            for (name, id) in CHANNELS {
                w.line(&format!("\"{name}\" => Some({id}),"));
            }
            w.line("_ => None,");
        });
    });
    w.finish().unwrap()
}

fn main() {
    let a = raw();
    let b = built();
    assert_eq!(a, b, "the two approaches must produce the same source");
    println!("identical output, {} lines", b.lines().count());
    print!("{b}");
}
