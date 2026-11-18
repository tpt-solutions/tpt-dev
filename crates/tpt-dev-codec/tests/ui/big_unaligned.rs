use tpt_dev_codec::{Field, Layout};

const FIELDS: [Field; 1] = [Field::be("x", 4, 8)];
const LAYOUT: Layout<2> = Layout::new(&FIELDS);

fn main() {
    let _ = LAYOUT;
}
