use tpt_dev_mock::mock;

#[mock]
pub trait Store {
    fn get<T: Clone>(&self, key: u8) -> T;
}

fn main() {}
