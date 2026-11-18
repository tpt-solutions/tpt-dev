use tpt_dev_lock::{L1, Mutex, Token};

fn main() {
    let a: Mutex<L1, u32> = Mutex::new(0);
    let b: Mutex<L1, u32> = Mutex::new(0);
    // Takes a level-1 lock while already holding one: rejected, since it could deadlock.
    a.lock_with(Token::root(), |_, held| b.lock_with(held, |_, _| ()));
}
