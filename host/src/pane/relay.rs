//! The pane's views in a shell that lives in another process (the Electron shell, over the sidecar's pipes): every call of the
//! [`Driver`] becomes an order (`wire::Order::Pane`), and the shell's answers and its views' events come back as notes
//! (`wire::ShellNote`). The rules stay in [`Pane`](super::Pane); this only carries them.

use super::{Driver, Place, View, ViewEvent};
use crate::wire::{Order, PaneOp, PaneView, ViewNote};
use crate::{BoxFuture, Res, lock};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

/// What a call fails with once the shell is gone.
const GONE: &str = "Faber's window is closed";

/// The wire's name for a view.
pub fn view_of(view: View) -> PaneView {
    match view {
        View::Main => PaneView::Main,
        View::Behind => PaneView::Behind,
    }
}

/// The view a note names.
pub fn view_named(view: PaneView) -> View {
    match view {
        PaneView::Main => View::Main,
        PaneView::Behind => View::Behind,
    }
}

/// What a note says a view told, as the pane hears it.
pub fn event_of(note: ViewNote) -> ViewEvent {
    match note {
        ViewNote::Started { url } => ViewEvent::Started { url },
        ViewNote::Finished { url } => ViewEvent::Finished { url },
        ViewNote::Moved { url } => ViewEvent::Moved { url },
        ViewNote::Titled => ViewEvent::Titled,
        ViewNote::Gone => ViewEvent::Gone,
    }
}

type Answer = oneshot::Sender<Res<Value>>;

struct Link {
    /// Calls the shell has not answered, by id; None once the shell is gone, so no call is sent after that.
    waiting: Option<HashMap<u64, Answer>>,
    /// The views the shell opened and the host has not closed.
    open: HashSet<View>,
    generation: HashMap<View, u64>,
}

/// A [`Driver`] whose views are the shell's, reached by orders.
pub struct Relay {
    send: Arc<dyn Fn(Order) + Send + Sync>,
    next: AtomicU64,
    /// An order is sent while this is held, so orders leave in the order their calls were polled.
    link: Mutex<Link>,
}

impl Relay {
    /// `send` carries an order to the shell (the process's one writer).
    pub fn new(send: Arc<dyn Fn(Order) + Send + Sync>) -> Arc<Relay> {
        Arc::new(Relay {
            send,
            next: AtomicU64::new(1),
            link: Mutex::new(Link { waiting: Some(HashMap::new()), open: HashSet::new(), generation: HashMap::new() }),
        })
    }

    /// The shell answered call `id`. An answer nobody waits for (a call the host gave up on, or an id never sent) changes
    /// nothing.
    pub fn answer(&self, id: u64, result: Res<Value>) {
        let waiting = lock(&self.link).waiting.as_mut().and_then(|waiting| waiting.remove(&id));
        if let Some(waiting) = waiting {
            let _ = waiting.send(result);
        }
    }

    /// The shell is gone: calls still waiting fail now, and every later one fails at once.
    pub fn lost(&self) {
        let waiting = {
            let mut link = lock(&self.link);
            link.open.clear();
            link.waiting.take()
        };
        for (_, waiting) in waiting.into_iter().flatten() {
            let _ = waiting.send(Err(GONE.into()));
        }
    }

    /// Sends a call when the future is first polled, and waits for the shell's answer. A future dropped before it ends (the
    /// host's deadline) forgets the call, so a late answer is dropped.
    fn ask(&self, view: View, op: PaneOp) -> BoxFuture<'_, Res<Value>> {
        Box::pin(async move {
            let (answer, answered) = oneshot::channel();
            let (id, generation) = {
                let mut link = lock(&self.link);
                if link.waiting.is_none() {
                    return Err(GONE.into());
                }
                if !matches!(op, PaneOp::Open { .. }) && !link.open.contains(&view) {
                    return Err("not open".into());
                }
                let generation = if matches!(op, PaneOp::Open { .. }) {
                    let generation = link.generation.entry(view).or_default();
                    *generation += 1;
                    Some(*generation)
                } else {
                    None
                };
                let id = self.next.fetch_add(1, Ordering::Relaxed);
                link.waiting.get_or_insert_with(HashMap::new).insert(id, answer);
                (self.send)(Order::Pane { id: Some(id), view: view_of(view), op });
                (id, generation)
            };
            let mut forget = Forget { relay: self, id: Some(id) };
            let result = answered.await.unwrap_or_else(|_| Err(GONE.into()));
            forget.id = None;
            if result.is_ok() {
                if let Some(generation) = generation {
                    let mut link = lock(&self.link);
                    if link.waiting.is_none() || link.generation.get(&view) != Some(&generation) {
                        return Err(GONE.into());
                    }
                    link.open.insert(view);
                }
            }
            result
        })
    }
}

/// Removes a call from the waiting ones when its future is dropped.
struct Forget<'a> {
    relay: &'a Relay,
    id: Option<u64>,
}

impl Drop for Forget<'_> {
    fn drop(&mut self) {
        if let Some(id) = self.id {
            if let Some(waiting) = lock(&self.relay.link).waiting.as_mut() {
                waiting.remove(&id);
            }
        }
    }
}

impl Driver for Relay {
    fn open(&self, view: View, profile: String) -> BoxFuture<'_, Res<()>> {
        Box::pin(async move { self.ask(view, PaneOp::Open { profile }).await.map(drop) })
    }

    fn close(&self, view: View) {
        let mut link = lock(&self.link);
        link.open.remove(&view);
        *link.generation.entry(view).or_default() += 1;
        // Always sent: an open the host gave up on may still have made the view.
        if link.waiting.is_some() {
            (self.send)(Order::Pane { id: None, view: view_of(view), op: PaneOp::Close });
        }
    }

    fn place(&self, view: View, place: Place) -> Res<()> {
        let link = lock(&self.link);
        if link.waiting.is_none() {
            return Err(GONE.into());
        }
        if !link.open.contains(&view) {
            return Err("not open".into());
        }
        let op = match place {
            Place::Parked => PaneOp::Park,
            Place::Over { x, y, width, height } => PaneOp::Place { x, y, width, height },
        };
        (self.send)(Order::Pane { id: None, view: view_of(view), op });
        Ok(())
    }

    fn load(&self, view: View, url: String) -> BoxFuture<'_, Res<()>> {
        Box::pin(async move { self.ask(view, PaneOp::Load { url }).await.map(drop) })
    }

    fn call(&self, view: View, method: &'static str, params: Value) -> BoxFuture<'_, Res<Value>> {
        self.ask(view, PaneOp::Call { method: method.into(), params })
    }

    fn clear(&self, view: View) -> BoxFuture<'_, Res<()>> {
        Box::pin(async move { self.ask(view, PaneOp::Clear).await.map(drop) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pane::{Home, Pane, PaneEvent};
    use serde_json::json;
    use std::time::Duration;
    use tokio::runtime::Handle;
    use tokio::sync::mpsc;

    type Orders = mpsc::UnboundedReceiver<Order>;

    fn relay() -> (Arc<Relay>, Orders) {
        let (out, orders) = mpsc::unbounded_channel();
        (Relay::new(Arc::new(move |order| drop(out.send(order)))), orders)
    }

    async fn next(orders: &mut Orders) -> Order {
        tokio::time::timeout(Duration::from_secs(5), orders.recv()).await.expect("an order").expect("the relay is alive")
    }

    fn pane_order(order: Order) -> (Option<u64>, PaneView, PaneOp) {
        match order {
            Order::Pane { id, view, op } => (id, view, op),
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn close_invalidates_a_late_open_and_precedes_a_replacement() {
        let (relay, mut orders) = relay();
        let first = tokio::spawn({
            let relay = relay.clone();
            async move { relay.open(View::Main, "first".into()).await }
        });
        let (id, _, _) = pane_order(next(&mut orders).await);
        relay.close(View::Main);
        assert_eq!(pane_order(next(&mut orders).await), (None, PaneView::Main, PaneOp::Close));
        let second = tokio::spawn({
            let relay = relay.clone();
            async move { relay.open(View::Main, "second".into()).await }
        });
        let (second_id, _, _) = pane_order(next(&mut orders).await);
        relay.answer(second_id.unwrap(), Ok(Value::Null));
        assert_eq!(second.await.unwrap(), Ok(()));
        relay.answer(id.unwrap(), Ok(Value::Null));
        assert!(first.await.unwrap().is_err());
        assert!(relay.place(View::Main, Place::Parked).is_ok());
        relay.close(View::Main);
        assert_eq!(pane_order(next(&mut orders).await), (None, PaneView::Main, PaneOp::Park));
        assert_eq!(pane_order(next(&mut orders).await), (None, PaneView::Main, PaneOp::Close));
        assert!(relay.place(View::Main, Place::Parked).is_err());
    }

    #[tokio::test]
    async fn a_late_open_answer_before_its_replacement_stays_closed() {
        let (relay, mut orders) = relay();
        let first = tokio::spawn({
            let relay = relay.clone();
            async move { relay.open(View::Main, "first".into()).await }
        });
        let (id, _, _) = pane_order(next(&mut orders).await);
        relay.close(View::Main);
        assert_eq!(pane_order(next(&mut orders).await), (None, PaneView::Main, PaneOp::Close));
        let second = tokio::spawn({
            let relay = relay.clone();
            async move { relay.open(View::Main, "second".into()).await }
        });
        let (second_id, _, _) = pane_order(next(&mut orders).await);
        relay.answer(id.unwrap(), Ok(Value::Null));
        assert!(first.await.unwrap().is_err());
        assert!(relay.place(View::Main, Place::Parked).is_err());
        relay.answer(second_id.unwrap(), Ok(Value::Null));
        assert!(second.await.unwrap().is_ok());
        assert!(relay.place(View::Main, Place::Parked).is_ok());
    }

    /// Opens `view` the way the pane does, with the shell answering at once.
    async fn opened(relay: &Arc<Relay>, orders: &mut Orders, view: View) {
        let opening = tokio::spawn({
            let relay = relay.clone();
            async move { relay.open(view, "faber-pane-x".into()).await }
        });
        let (id, shown, op) = pane_order(next(orders).await);
        assert_eq!((shown, op), (view_of(view), PaneOp::Open { profile: "faber-pane-x".into() }));
        relay.answer(id.expect("a call has an id"), Ok(Value::Null));
        opening.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn a_call_is_ordered_in_the_order_it_is_polled_and_answered_under_its_id() {
        let (relay, mut orders) = relay();
        opened(&relay, &mut orders, View::Main).await;
        let first = tokio::spawn({
            let relay = relay.clone();
            async move { relay.call(View::Main, "Page.getFrameTree", json!({ "a": 1 })).await }
        });
        let (first_id, view, op) = pane_order(next(&mut orders).await);
        assert_eq!((view, op), (PaneView::Main, PaneOp::Call { method: "Page.getFrameTree".into(), params: json!({ "a": 1 }) }));
        let second = tokio::spawn({
            let relay = relay.clone();
            async move { relay.load(View::Main, "https://example.org/".into()).await }
        });
        let (second_id, _, op) = pane_order(next(&mut orders).await);
        assert_eq!(op, PaneOp::Load { url: "https://example.org/".into() });
        assert_ne!(first_id, second_id);
        // Answers may come in any order, and one for an id nobody asked changes nothing.
        relay.answer(999, Ok(json!("stray")));
        relay.answer(second_id.unwrap(), Ok(Value::Null));
        assert_eq!(second.await.unwrap(), Ok(()));
        relay.answer(first_id.unwrap(), Ok(json!({ "frameTree": 1 })));
        assert_eq!(first.await.unwrap(), Ok(json!({ "frameTree": 1 })));
        let clearing = tokio::spawn({
            let relay = relay.clone();
            async move { relay.clear(View::Main).await }
        });
        let (id, _, op) = pane_order(next(&mut orders).await);
        assert_eq!(op, PaneOp::Clear);
        relay.answer(id.unwrap(), Err("the page said no".into()));
        assert_eq!(clearing.await.unwrap(), Err("the page said no".into()), "a failure of the shell reaches the caller as it is");
    }

    #[tokio::test]
    async fn placing_and_closing_answer_nothing_and_a_view_that_is_not_open_takes_no_call() {
        let (relay, mut orders) = relay();
        assert_eq!(relay.place(View::Main, Place::Parked), Err("not open".into()));
        assert_eq!(relay.load(View::Main, "https://example.org/".into()).await, Err("not open".into()));
        assert_eq!(relay.call(View::Behind, "Page.reload", json!({})).await, Err("not open".into()));
        assert!(orders.try_recv().is_err(), "nothing was ordered for a view that is not open");
        opened(&relay, &mut orders, View::Main).await;
        relay.place(View::Main, Place::Over { x: 1.0, y: 2.0, width: 30.0, height: 40.0 }).unwrap();
        relay.place(View::Main, Place::Parked).unwrap();
        relay.close(View::Main);
        assert_eq!(
            [pane_order(next(&mut orders).await), pane_order(next(&mut orders).await), pane_order(next(&mut orders).await)],
            [
                (None, PaneView::Main, PaneOp::Place { x: 1.0, y: 2.0, width: 30.0, height: 40.0 }),
                (None, PaneView::Main, PaneOp::Park),
                (None, PaneView::Main, PaneOp::Close),
            ]
        );
        assert_eq!(relay.place(View::Main, Place::Parked), Err("not open".into()), "closed");
        // A close is always ordered, even for a view an open the host gave up on may have made.
        relay.close(View::Behind);
        assert_eq!(pane_order(next(&mut orders).await), (None, PaneView::Behind, PaneOp::Close));
    }

    #[tokio::test]
    async fn a_call_the_host_gave_up_on_is_forgotten_and_its_late_answer_changes_nothing() {
        let (relay, mut orders) = relay();
        opened(&relay, &mut orders, View::Main).await;
        let late = tokio::time::timeout(Duration::from_millis(20), relay.call(View::Main, "Page.reload", json!({}))).await;
        assert!(late.is_err(), "nobody answered");
        let (id, _, _) = pane_order(next(&mut orders).await);
        relay.answer(id.unwrap(), Ok(json!("late")));
        assert!(lock(&relay.link).waiting.as_ref().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_lost_shell_fails_the_calls_waiting_and_every_call_after() {
        let (relay, mut orders) = relay();
        opened(&relay, &mut orders, View::Main).await;
        let waiting = tokio::spawn({
            let relay = relay.clone();
            async move { relay.call(View::Main, "Page.reload", json!({})).await }
        });
        next(&mut orders).await;
        relay.lost();
        assert_eq!(waiting.await.unwrap(), Err(GONE.into()));
        assert_eq!(relay.call(View::Main, "Page.reload", json!({})).await, Err(GONE.into()));
        assert_eq!(relay.open(View::Main, "p".into()).await, Err(GONE.into()));
        assert_eq!(relay.place(View::Main, Place::Parked), Err(GONE.into()));
        relay.close(View::Main);
        assert!(orders.try_recv().is_err(), "no order leaves for a shell that is gone");
    }

    /// The home a pane needs when it has no engine: it keeps what the screen is told.
    struct Told(Mutex<Vec<PaneEvent>>);

    impl Home for Told {
        fn tabs(&self, _: String) -> BoxFuture<'_, Option<faber_engine::browser::tabs::Tabs>> {
            Box::pin(async { None })
        }
        fn tab(&self, _: String, _: faber_engine::TabStep) -> BoxFuture<'_, Option<(faber_engine::browser::tabs::Tabs, Option<String>)>> {
            Box::pin(async { Some((faber_engine::browser::tabs::Tabs::default(), Some("t1".to_string()))) })
        }
        fn seen(&self, _: String, _: Option<String>, _: String, _: Option<String>) -> BoxFuture<'_, bool> {
            Box::pin(async { false })
        }
        fn tell(&self, event: PaneEvent) {
            lock(&self.0).push(event);
        }
    }

    #[tokio::test]
    async fn what_a_view_tells_reaches_the_pane_and_a_gone_view_closes_it() {
        let (relay, mut orders) = relay();
        let home = Arc::new(Told(Mutex::default()));
        let pane = Pane::new(relay.clone(), home.clone(), Handle::current());
        // The person opens a card's page: the pane opens its view and loads the page, the shell answering each.
        let showing = tokio::spawn({
            let pane = pane.clone();
            async move { pane.site_open("c".into(), "https://example.org/a".into()).await }
        });
        for expected in ["open", "load"] {
            let (id, view, op) = pane_order(next(&mut orders).await);
            assert_eq!(view, PaneView::Main);
            assert!(matches!((expected, &op), ("open", PaneOp::Open { .. }) | ("load", PaneOp::Load { .. })), "{op:?}");
            relay.answer(id.unwrap(), Ok(Value::Null));
        }
        assert_eq!(showing.await.unwrap(), Ok(true));
        let page = "https://example.org/a".to_string();
        pane.event(view_named(PaneView::Main), event_of(ViewNote::Started { url: page.clone() }));
        pane.event(view_named(PaneView::Main), event_of(ViewNote::Finished { url: page.clone() }));
        let told = || lock(&home.0).iter().filter(|event| !matches!(event, PaneEvent::Tabs(_))).cloned().collect::<Vec<_>>();
        assert_eq!(told(), [PaneEvent::Loading(true), PaneEvent::Loading(false), PaneEvent::Page(page)]);
        pane.event(View::Main, event_of(ViewNote::Moved { url: "https://example.org/a#part".into() }));
        assert_eq!(told().last(), Some(&PaneEvent::Page("https://example.org/a#part".into())));
        // The view is gone (its window closed): the pane closes it and opens a new one next time.
        pane.event(View::Main, event_of(ViewNote::Gone));
        assert_eq!(pane_order(next(&mut orders).await), (None, PaneView::Main, PaneOp::Close));
        // An event of a view that is closed changes nothing.
        pane.event(View::Main, event_of(ViewNote::Finished { url: "https://example.org/b".into() }));
        assert_eq!(told().len(), 4);
    }
}
