use tpt_dev_mock::mock;

#[mock(Value = u8)]
pub trait Holder {
    fn get(&self) -> u8;
}

fn main() {}
