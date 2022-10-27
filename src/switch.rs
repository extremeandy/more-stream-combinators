use futures::Stream;
use pin_project_lite::pin_project;
use std::{task::Poll, time::Duration};

use crate::sample::{sample, Sample};

pub trait StreamExt: Stream {
    fn switch(self) -> SwitchStream<Self>
    where
        Self::Item: Stream,
        Self: Sized,
    {
        let stream = SwitchStream::new(self);
        assert_stream::<<Self::Item as Stream>::Item, _>(stream)
    }

    fn sample<S: Stream>(self, sampler: S) -> Sample<Self, S>
    where
        Self: Sized,
    {
        let stream = sample(self, sampler);
        assert_stream(stream)
    }
}

impl<T: Stream> StreamExt for T {}

pin_project! {
    #[must_use = "streams do nothing unless polled"]
    pub struct SwitchStream<St>
    where
        St: Stream,
        St::Item: Stream
    {
        #[pin]
        stream: St,

        #[pin]
        inner: Option<<St as Stream>::Item>
    }
}

impl<St> SwitchStream<St>
where
    St: Stream,
    St::Item: Stream,
{
    pub(super) fn new(stream: St) -> Self {
        Self {
            stream,
            inner: None,
        }
    }
}

impl<St> Stream for SwitchStream<St>
where
    St: Stream,
    St::Item: Stream,
{
    type Item = <St::Item as Stream>::Item;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let mut this = self.project();

        while let Poll::Ready(inner) = this.stream.as_mut().poll_next(cx) {
            match inner {
                Some(inner) => this.inner.set(Some(inner)),
                None => {
                    // Terminate when the outer stream terminates
                    return Poll::Ready(None);
                }
            }
        }

        match this.inner.as_mut().as_pin_mut() {
            Some(inner) => match inner.poll_next(cx) {
                Poll::Ready(value) => match value {
                    Some(value) => Poll::Ready(Some(value)),

                    // The inner stream can terminate but we don't terminate until the outer stream ends.
                    None => Poll::Pending,
                },

                // Waiting on inner stream to emit next
                Poll::Pending => Poll::Pending,
            },

            // We are still waiting for the first inner stream to be emitted by the outer
            None => Poll::Pending,
        }
    }
}

impl<St> std::fmt::Debug for SwitchStream<St>
where
    St: Stream + std::fmt::Debug,
    St::Item: Stream + std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SwitchStream")
            .field("stream", &self.stream)
            .field("inner", &self.inner)
            .finish()
    }
}

// Just a helper function to ensure the streams we're returning all have the
// right implementations.
pub(crate) fn assert_stream<T, S>(stream: S) -> S
where
    S: Stream<Item = T>,
{
    stream
}

#[tokio::test]
async fn test_switch() {
    use futures::{channel::mpsc, SinkExt, StreamExt};
    use tokio::sync::broadcast::{self, error::SendError};
    use tokio_stream::wrappers::BroadcastStream;

    let waker = futures::task::noop_waker_ref();
    let mut cx = std::task::Context::from_waker(waker);

    let (tx_inner1, rx_inner1) = broadcast::channel(32);
    let (tx_inner2, rx_inner2) = broadcast::channel(32);
    let (tx_inner3, rx_inner3) = broadcast::channel(32);
    let (mut tx, rx) = mpsc::unbounded();

    let mut switch_stream = rx.switch().peekable();
    assert_eq!(switch_stream.poll_next_unpin(&mut cx), Poll::Pending);

    tx.send(
        BroadcastStream::new(rx_inner1)
            .map(|r: Result<_, _>| r.unwrap())
            .boxed(),
    )
    .await
    .unwrap();
    assert_eq!(switch_stream.poll_next_unpin(&mut cx), Poll::Pending);

    tx_inner1.send(10).unwrap();
    assert_eq!(
        switch_stream.poll_next_unpin(&mut cx),
        Poll::Ready(Some(10))
    );
    assert_eq!(switch_stream.poll_next_unpin(&mut cx), Poll::Pending);

    tx_inner1.send(20).unwrap();
    assert_eq!(
        switch_stream.poll_next_unpin(&mut cx),
        Poll::Ready(Some(20))
    );

    tx.send(
        BroadcastStream::new(rx_inner2)
            .map(|r: Result<_, _>| r.unwrap())
            .boxed(),
    )
    .await
    .unwrap();

    assert_eq!(switch_stream.poll_next_unpin(&mut cx), Poll::Pending);

    // We expect trying to send to the first inner stream to fail because
    // rx_inner1 should have been dropped by SwitchStream once we started
    // listening to rx_inner2.
    matches!(tx_inner1.send(30), Err(SendError(_)));
    assert_eq!(switch_stream.poll_next_unpin(&mut cx), Poll::Pending);

    // This should not cause the result stream to terminate.
    // We only terminate on the outer stream terminating.
    drop(tx_inner2);
    assert_eq!(switch_stream.poll_next_unpin(&mut cx), Poll::Pending);

    tx.send(
        BroadcastStream::new(rx_inner3)
            .map(|r: Result<_, _>| r.unwrap())
            .boxed(),
    )
    .await
    .unwrap();

    tx_inner3.send(100).unwrap();
    assert_eq!(
        switch_stream.poll_next_unpin(&mut cx),
        Poll::Ready(Some(100))
    );

    tx_inner3.send(110).unwrap();
    assert_eq!(
        switch_stream.poll_next_unpin(&mut cx),
        Poll::Ready(Some(110))
    );

    drop(tx);
    assert_eq!(switch_stream.poll_next_unpin(&mut cx), Poll::Ready(None));
}
