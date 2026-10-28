use std::collections::HashMap;
use std::future::poll_fn;
use std::task::Poll;
use tokio::sync::oneshot;

pub(crate) struct PendingRequests<T> {
    next_id: i64,
    replies: HashMap<i64, oneshot::Sender<T>>,
}

impl<T> PendingRequests<T> {
    pub(crate) fn starting_at(next_id: i64) -> Self {
        Self { next_id, replies: HashMap::new() }
    }

    pub(crate) fn insert(&mut self, reply: oneshot::Sender<T>) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        self.replies.insert(id, reply);
        id
    }

    pub(crate) fn resolve(&mut self, id: i64, value: T) {
        if let Some(reply) = self.replies.remove(&id) {
            let _ = reply.send(value);
        }
    }

    pub(crate) fn fail_all(&mut self, error: impl Fn() -> T) {
        for (_, reply) in self.replies.drain() {
            let _ = reply.send(error());
        }
    }

    pub(crate) async fn abandoned(&mut self) -> i64 {
        let id = poll_fn(|cx| {
            self.replies
                .iter_mut()
                .find_map(|(id, reply)| reply.poll_closed(cx).is_ready().then_some(*id))
                .map_or(Poll::Pending, Poll::Ready)
        })
        .await;
        self.replies.remove(&id);
        id
    }
}
