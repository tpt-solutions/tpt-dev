use tpt_dev_codec::{Field, Layout};

const FIELDS: [Field; 2] = [Field::bits("a", 0, 4), Field::bits("b", 3, 4)];
const LAYOUT: Layout<2> = Layout::new(&FIELDS);

fn main() {
    let _ = LAYOUT;
}
