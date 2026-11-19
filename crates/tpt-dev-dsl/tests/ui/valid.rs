use tpt_dev_dsl::dsl;

dsl! {
    pub SIMPLE {
        start = "a" / "b" rest*;
        rest = ['0'-'9'] "" / "c"?;
    }
}

fn main() {
    assert!(SIMPLE.recognise(0, b"a"));
    assert!(SIMPLE.recognise(0, b"b7c"));
}
