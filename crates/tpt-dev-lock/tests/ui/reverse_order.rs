use tpt_dev_lock::{L1, L2, Mutex, Token};

fn main() {
    let outer: Mutex<L1, u32> = Mutex::new(0);
    let inner: Mutex<L2, u32> = Mutex::new(0);
    // Takes the level-2 lock first, then tries the level-1 lock: rejected.
    inner.lock_with(Token::root(), |_, held| outer.lock_with(held, |_, _| ()));
}
