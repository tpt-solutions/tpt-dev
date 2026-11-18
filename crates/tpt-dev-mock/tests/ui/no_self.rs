use tpt_dev_mock::mock;

#[mock]
pub trait Factory {
    fn make() -> u8;
}

fn main() {}
