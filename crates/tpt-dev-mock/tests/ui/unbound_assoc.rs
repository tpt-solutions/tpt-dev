use tpt_dev_mock::mock;

#[mock]
pub trait Holder {
    type Item;
    fn get(&self) -> Self::Item;
}

fn main() {}
