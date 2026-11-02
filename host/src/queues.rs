//! The two kinds of read the screen fires faster than they finish: the message search, where only the latest query waits for
//! the one scan that runs, and the prompt history, where a blur cancels the read it started.

use crate::lock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

struct SearchRequest {
    replaced: tokio::sync::oneshot::Sender<()>,
    cancelled: Arc<AtomicBool>,
}

pub(crate) struct SearchQueue {
    scanning: tokio::sync::Mutex<()>,
    latest: Mutex<Option<SearchRequest>>,
}

impl SearchQueue {
    pub(crate) const fn new() -> Self {
        Self { scanning: tokio::sync::Mutex::const_new(()), latest: Mutex::new(None) }
    }

    pub(crate) async fn acquire(&self) -> Option<(tokio::sync::MutexGuard<'_, ()>, Arc<AtomicBool>)> {
        let (replaced, gone) = tokio::sync::oneshot::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        if let Some(older) = lock(&self.latest).replace(SearchRequest { replaced, cancelled: cancelled.clone() }) {
            older.cancelled.store(true, Ordering::Relaxed);
            let _ = older.replaced.send(());
        }
        tokio::select! {
            biased;
            _ = gone => None,
            one = self.scanning.lock() => Some((one, cancelled)),
        }
    }
}

#[derive(Default)]
pub(crate) struct PromptReads {
    active: Option<(String, Arc<AtomicBool>)>,
    // A blur may arrive before its read starts on the async worker. Retain only recent request ids, never prompt text.
    cancelled: std::collections::VecDeque<String>,
}

impl PromptReads {
    pub(crate) fn start(&mut self, id: String) -> Option<Arc<AtomicBool>> {
        if self.cancelled.contains(&id) {
            return None;
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        if let Some((_, old)) = self.active.replace((id, cancelled.clone())) {
            old.store(true, Ordering::Relaxed);
        }
        Some(cancelled)
    }

    pub(crate) fn cancel(&mut self, id: String) {
        if self.active.as_ref().is_some_and(|(active, _)| *active == id)
            && let Some((_, token)) = self.active.take()
        {
            token.store(true, Ordering::Relaxed);
        }
        self.cancelled.push_back(id);
        if self.cancelled.len() > 32 {
            self.cancelled.pop_front();
        }
    }

    pub(crate) fn finish(&mut self, id: &str) {
        if self.active.as_ref().is_some_and(|(active, _)| active == id) {
            self.active = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Ordering, PromptReads, SearchQueue};
    use std::future::Future;
    use std::task::{Context, Poll, Waker};

    #[test]
    fn prompt_history_cancels_superseded_reads_and_only_the_blurred_request() {
        let mut reads = PromptReads::default();
        let first = reads.start("first".into()).unwrap();
        let second = reads.start("second".into()).unwrap();
        assert!(first.load(Ordering::Relaxed));
        reads.cancel("first".into());
        reads.finish("first");
        assert!(!second.load(Ordering::Relaxed));
        reads.cancel("second".into());
        assert!(second.load(Ordering::Relaxed));
    }

    #[test]
    fn prompt_history_a_blur_before_worker_start_prevents_the_read_with_bounded_request_ids() {
        let mut reads = PromptReads::default();
        for id in 0..40 {
            reads.cancel(id.to_string());
        }
        assert_eq!(reads.cancelled.len(), 32);
        assert!(reads.start("39".into()).is_none());
        let token = reads.start("new".into()).unwrap();
        reads.finish("new");
        assert!(reads.active.is_none());
        assert!(!token.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn searches_cancel_the_running_scan_and_replace_only_the_waiting_query() {
        let queue = SearchQueue::new();
        let (running, cancelled) = queue.acquire().await.unwrap();
        let mut context = Context::from_waker(Waker::noop());
        let mut waiting = std::pin::pin!(queue.acquire());
        assert!(waiting.as_mut().poll(&mut context).is_pending());
        assert!(cancelled.load(Ordering::Relaxed), "the running scan must stop before reading another file");

        let mut newest = std::pin::pin!(queue.acquire());
        assert!(newest.as_mut().poll(&mut context).is_pending(), "the newer scan must wait for the running scan");
        assert!(matches!(waiting.as_mut().poll(&mut context), Poll::Ready(None)), "the replaced query must give up without scanning");
        drop(running);

        let (running, cancelled) = newest.await.unwrap();
        assert!(!cancelled.load(Ordering::Relaxed), "only the latest query gets to scan");
        drop(running);
        let (_, cancelled) = queue.acquire().await.unwrap();
        assert!(!cancelled.load(Ordering::Relaxed), "an idle queue must admit the next query");
    }
}
