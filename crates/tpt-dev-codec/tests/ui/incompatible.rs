use tpt_dev_codec::{Field, Layout};

const V1_FIELDS: [Field; 2] = [Field::bits("id", 0, 8), Field::bits("flags", 8, 8)];
const V2_FIELDS: [Field; 2] = [Field::bits("id", 0, 4), Field::bits("flags", 8, 8)];
const V1: Layout<2> = Layout::new(&V1_FIELDS);
const V2: Layout<2> = Layout::new(&V2_FIELDS).with_version(2);
const _: () = assert!(V2.extends(&V1));

fn main() {}
