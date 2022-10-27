//! Slow down a stream by enforcing a delay between items.

use futures::future::BoxFuture;
use futures::stream::{FuturesOrdered, FuturesUnordered};
use futures::{ready, FutureExt, Stream, StreamExt};
use pin_project_lite::pin_project;
use std::future::Future;
use std::marker::Unpin;
use std::ops::Deref;
use std::pin::Pin;
use std::task::{self, Poll};
use tokio::time::{Duration, Instant, Sleep};

pub(super) fn sample<T>(duration: Duration, stream: T) -> Sample<T>
where
    T: Stream,
{
    Sample {
        delay: tokio::time::sleep_until(Instant::now() + duration),
        duration,
        has_delayed: true,
        stream,
        last_value: None,
        finished: false,
    }
}

pin_project! {
    /// Stream for the [`sample`](sample) function. This object is `!Unpin`. If you need it to
    /// implement `Unpin` you can pin your sample like this: `Box::pin(your_sample)`.
    #[derive(Debug)]
    #[must_use = "streams do nothing unless polled"]
    pub struct Sample<T: Stream> {
        #[pin]
        delay: Sleep,

        duration: Duration,

        // Set to true when `delay` has returned ready, but `stream` hasn't.
        has_delayed: bool,

        // The stream to sample
        #[pin]
        stream: T,

        last_value: Option<T::Item>,

        // Set to true when `stream` completed in the last call to `poll_next`
        // but we yielded `Poll::Ready(Some(_))` because `stream` yielded `Some`
        // over the sampling duration
        finished: bool
    }
}

// XXX: are these safe if `T: !Unpin`?
impl<T: Stream + Unpin> Sample<T> {
    /// Acquires a reference to the underlying stream that this combinator is
    /// pulling from.
    pub fn get_ref(&self) -> &T {
        &self.stream
    }

    /// Acquires a mutable reference to the underlying stream that this combinator
    /// is pulling from.
    ///
    /// Note that care must be taken to avoid tampering with the state of the stream
    /// which may otherwise confuse this combinator.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.stream
    }

    /// Consumes this combinator, returning the underlying stream.
    ///
    /// Note that this may discard intermediate state of this combinator, so care
    /// should be taken to avoid losing resources when this is called.
    pub fn into_inner(self) -> T {
        self.stream
    }
}

impl<T: Stream> Stream for Sample<T> {
    type Item = T::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Option<Self::Item>> {
        let mut me = self.project();

        if *me.finished {
            return Poll::Ready(None);
        }

        let dur = *me.duration;

        if !*me.has_delayed && !dur.is_zero() {
            ready!(me.delay.as_mut().poll(cx));
            *me.has_delayed = true;
        }

        let mut value = ready!(me.stream.poll_next_unpin(cx));

        if value.is_some() {
            if !dur.is_zero() {
                let next_deadline = me.delay.deadline() + dur;
                me.delay.reset(next_deadline);

                while let Poll::Ready(inner) = me.stream.poll_next_unpin(cx) {
                    //println!("Skip!");
                    match inner {
                        Some(inner) => value = Some(inner),
                        None => *me.finished = true,
                    }

                    if let Poll::Ready(_) = me.delay.as_mut().poll(cx) {
                        break;
                    }
                    // Err....
                    // if Instant::now() > next_deadline {
                    //     break;
                    // }
                }
            }

            *me.has_delayed = false;
        }

        Poll::Ready(value)
    }
}
