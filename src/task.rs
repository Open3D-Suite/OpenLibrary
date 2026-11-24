// SPDX-License-Identifier: GPL-3.0-or-later
//! Work on a thread of its own, whose messages and result the window collects frame by frame.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

/// What the window is called with when there is something new to draw.
pub type Wake = Arc<dyn Fn() + Send + Sync>;

/// The worker's side: messages to the window, and whether to stop.
pub struct Link<T> {
    send: Sender<T>,
    cancel: Arc<AtomicBool>,
    wake: Wake,
}

impl<T> Link<T> {
    pub fn send(&self, message: T) {
        let _ = self.send.send(message); // the window may have stopped listening
        (self.wake)();
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// The window's side. Dropping it cancels the work and waits for it to stop.
pub struct Task<T> {
    receive: Receiver<T>,
    cancel: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl<T: Send + 'static> Task<T> {
    pub fn start(wake: Wake, work: impl FnOnce(&Link<T>) + Send + 'static) -> Self {
        let (send, receive) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let link = Link { send, cancel: cancel.clone(), wake };
        let thread = Some(std::thread::spawn(move || work(&link)));
        Self { receive, cancel, thread }
    }
}

impl<T> Task<T> {
    /// The messages since the last call.
    pub fn poll(&self) -> Vec<T> {
        self.receive.try_iter().collect()
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

impl<T> Drop for Task<T> {
    fn drop(&mut self) {
        self.cancel();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_arrive_and_dropping_stops_the_work() {
        let (started, wait) = channel();
        let task = Task::start(Arc::new(|| {}), move |link| {
            link.send(1);
            link.send(2);
            let _ = started.send(());
            while !link.cancelled() {
                std::thread::yield_now();
            }
        });
        wait.recv().unwrap();
        assert_eq!(task.poll(), [1, 2]);
        assert!(!task.cancelled());
        drop(task); // would hang if it did not cancel
    }
}
