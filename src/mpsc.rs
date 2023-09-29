pub use calloop::channel::{Channel as Receiver, Sender};

pub fn channel<T>(_: u32) -> (Sender<T>, Receiver<T>) {
    calloop::channel::channel()
}
