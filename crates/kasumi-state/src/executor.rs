//! A tiny background executor for resource loaders.
//!
//! Loaders are blocking closures or futures. They run on a small shared pool so
//! the UI thread never waits for I/O. Futures are driven with a thread-unpark
//! waker; they need their own reactor if they perform async I/O (a raw future
//! that waits for an external runtime cannot make progress here).

use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::task::{Context, Poll, Wake, Waker};
use std::thread;

type Job = Box<dyn FnOnce() + Send + 'static>;

fn sender() -> &'static mpsc::Sender<Job> {
    static SENDER: OnceLock<mpsc::Sender<Job>> = OnceLock::new();
    SENDER.get_or_init(|| {
        let (sender, receiver) = mpsc::channel::<Job>();
        let receiver = Arc::new(Mutex::new(receiver));
        let workers = thread::available_parallelism()
            .map(|count| count.get())
            .unwrap_or(4)
            .clamp(2, 4);
        for index in 0..workers {
            let receiver = Arc::clone(&receiver);
            let spawned = thread::Builder::new()
                .name(format!("kasumi-resource-{index}"))
                .spawn(move || {
                    loop {
                        // Only one worker waits on the channel at a time, but
                        // the lock is released before the job runs. That is
                        // enough to hand jobs out concurrently.
                        let job = match receiver.lock() {
                            Ok(guard) => guard.recv(),
                            Err(poisoned) => poisoned.into_inner().recv(),
                        };
                        let Ok(job) = job else {
                            break;
                        };
                        if catch_unwind(AssertUnwindSafe(job)).is_err() {
                            eprintln!("kasumi: a resource loader panicked");
                        }
                    }
                });
            if let Err(error) = spawned {
                eprintln!("kasumi: failed to spawn a resource worker: {error}");
            }
        }
        sender
    })
}

pub(crate) fn spawn(job: impl FnOnce() + Send + 'static) {
    if let Err(error) = sender().send(Box::new(job)) {
        eprintln!("kasumi: the resource executor stopped: {error}");
    }
}

/// Drives one future to completion on the current thread.
///
/// Used by future-backed resources. A future that requires a platform or I/O
/// reactor must bring one with it; this scheduler only polls and parks.
pub(crate) fn block_on<F: Future>(future: F) -> F::Output {
    struct ThreadWaker(thread::Thread);

    impl Wake for ThreadWaker {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }

    let waker = Waker::from(Arc::new(ThreadWaker(thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => thread::park(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_on_completes_a_ready_future() {
        assert_eq!(block_on(async { 7u32 }), 7);
    }

    #[test]
    fn spawned_jobs_run_off_thread() {
        let (sender, receiver) = mpsc::channel();
        spawn(move || {
            let _ = sender.send(42u32);
        });
        assert_eq!(
            receiver.recv_timeout(std::time::Duration::from_secs(2)),
            Ok(42)
        );
    }
}
