//! One lazy, serialized native read. No worker exists until the returned future is polled.
use faber_engine::desktop::{Desktop, Observation};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Default)]
pub struct NativeDesktop {
    busy: Arc<AtomicBool>,
}

impl Desktop for NativeDesktop {
    fn read(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Observation, String>> + Send>> {
        Box::pin(read_once(self.busy.clone(), || {
            let read = faber_winproc::desktop::read_foreground().map_err(|_| "Desktop observation is unavailable.".to_string())?;
            Ok(Observation { title: read.title, text: read.text, truncated: read.truncated })
        }))
    }
}

struct Lease(Arc<AtomicBool>);
impl Drop for Lease {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

struct Abort(tokio::task::AbortHandle);
impl Drop for Abort {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn read_once<F>(busy: Arc<AtomicBool>, read: F) -> Result<Observation, String>
where
    F: FnOnce() -> Result<Observation, String> + Send + 'static,
{
    busy.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .map_err(|_| "Desktop observation is already running.".to_string())?;
    let lease = Lease(busy);
    let worker = tokio::task::spawn_blocking(move || {
        // The worker owns the lease even after its waiter is cancelled, including unwinding on panic.
        let _lease = lease;
        read()
    });
    // Abort a queued callback on future drop; Tokio cannot abort one that has already started.
    let _abort = Abort(worker.abort_handle());
    worker.await.map_err(|_| "Desktop observation failed.".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;
    fn observation() -> Observation {
        Observation { title: "fixture".into(), text: "visible".into(), truncated: true }
    }
    async fn started(rx: tokio::sync::oneshot::Receiver<()>) {
        tokio::time::timeout(Duration::from_secs(2), rx).await.expect("worker starts").unwrap();
    }

    #[tokio::test]
    async fn native_desktop_worker_is_lazy_and_runs_off_async_thread() {
        let busy = Arc::new(AtomicBool::new(false));
        let caller = std::thread::current().id();
        let future = read_once(busy.clone(), move || {
            assert_ne!(std::thread::current().id(), caller);
            Ok(observation())
        });
        assert!(!busy.load(Ordering::Acquire));
        assert_eq!(future.await.unwrap(), observation());
        assert!(!busy.load(Ordering::Acquire));
    }

    #[tokio::test]
    async fn native_desktop_worker_error_and_panic_release_lease() {
        let busy = Arc::new(AtomicBool::new(false));
        assert_eq!(read_once(busy.clone(), || Err("fixture failure".into())).await, Err("fixture failure".into()));
        assert!(!busy.load(Ordering::Acquire));
        let called = Arc::new(AtomicBool::new(false));
        let callback_called = called.clone();
        assert!(
            read_once(busy.clone(), move || {
                callback_called.store(true, Ordering::Release);
                panic!("fixture panic")
            })
            .await
            .is_err()
        );
        assert!(called.load(Ordering::Acquire));
        assert!(!busy.load(Ordering::Acquire));
        assert_eq!(read_once(busy, || Ok(observation())).await.unwrap(), observation());
    }

    #[test]
    fn native_desktop_worker_cancel_running_keeps_lease_until_exit() {
        tokio::runtime::Builder::new_current_thread().enable_time().max_blocking_threads(1).build().unwrap().block_on(async {
            let busy = Arc::new(AtomicBool::new(false));
            let (start, ready) = tokio::sync::oneshot::channel();
            let (release, held) = mpsc::channel();
            let (finish, ended) = tokio::sync::oneshot::channel();
            let worker = tokio::spawn(read_once(busy.clone(), move || {
                start.send(()).unwrap();
                held.recv().unwrap();
                finish.send(()).unwrap();
                Ok(observation())
            }));
            started(ready).await;
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(2), read_once(busy.clone(), || panic!("second callback must not run")))
                    .await
                    .unwrap(),
                Err("Desktop observation is already running.".into())
            );
            worker.abort();
            assert!(worker.await.unwrap_err().is_cancelled());
            assert!(busy.load(Ordering::Acquire), "cancelling the waiter cannot release a running worker");
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(2), read_once(busy.clone(), || panic!("cancelled callback still owns lease")))
                    .await
                    .unwrap(),
                Err("Desktop observation is already running.".into())
            );
            release.send(()).unwrap();
            started(ended).await;
            // Joining a later blocking callback proves the earlier callback and its lease have dropped.
            tokio::task::spawn_blocking(|| {}).await.unwrap();
            assert!(!busy.load(Ordering::Acquire));
            assert_eq!(read_once(busy, || Ok(observation())).await.unwrap(), observation());
        });
    }
    #[test]
    fn native_desktop_worker_cancel_queued_never_calls_callback() {
        tokio::runtime::Builder::new_current_thread().enable_time().max_blocking_threads(1).build().unwrap().block_on(async {
            let (start, ready) = tokio::sync::oneshot::channel();
            let (release, held) = mpsc::channel();
            let occupied = tokio::task::spawn_blocking(move || {
                start.send(()).unwrap();
                held.recv().unwrap();
            });
            started(ready).await;
            let busy = Arc::new(AtomicBool::new(false));
            let called = Arc::new(AtomicBool::new(false));
            let callback_called = called.clone();
            let mut read = Box::pin(read_once(busy.clone(), move || {
                callback_called.store(true, Ordering::Release);
                Ok(observation())
            }));
            let state = std::future::poll_fn(|cx| std::task::Poll::Ready(std::future::Future::poll(read.as_mut(), cx))).await;
            assert!(state.is_pending(), "queued read awaits its blocking worker");
            assert!(busy.load(Ordering::Acquire));
            drop(read);
            release.send(()).unwrap();
            occupied.await.unwrap();
            tokio::task::spawn_blocking(|| {}).await.unwrap();
            assert!(!called.load(Ordering::Acquire), "cancelled queued callback cannot observe a later foreground window");
            assert!(!busy.load(Ordering::Acquire));
        });
    }
}
