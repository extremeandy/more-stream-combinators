//! Slow down a stream by enforcing a delay between items.

use futures::Stream;
use pin_project_lite::pin_project;
use std::marker::Unpin;
use std::pin::Pin;
use std::task::{self, Poll};

pub(super) fn sample<T, S>(stream: T, sampler: S) -> Sample<T, S>
where
    T: Stream,
{
    Sample {
        stream,
        sampler,
        value: None,
    }
}

pin_project! {
    /// Stream for the [`sample`](sample) function. This object is `!Unpin`. If you need it to
    /// implement `Unpin` you can pin your sample like this: `Box::pin(your_sample)`.
    #[derive(Debug)]
    #[must_use = "streams do nothing unless polled"]
    pub struct Sample<T: Stream, S> {
        // The stream to sample
        #[pin]
        stream: T,

        #[pin]
        sampler: S,

        value: Option<T::Item>
    }
}

// XXX: are these safe if `T: !Unpin`?
impl<T: Stream + Unpin, S> Sample<T, S> {
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

impl<T: Stream, S: Stream> Stream for Sample<T, S> {
    type Item = T::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();

        let mut prev_value = None;
        match this.stream.poll_next(cx) {
            Poll::Ready(Some(value)) => {
                prev_value = this.value.replace(value);
            }
            Poll::Ready(None) => return Poll::Ready(None),
            Poll::Pending => {}
        }

        match this.sampler.poll_next(cx) {
            Poll::Ready(Some(_)) => match prev_value {
                Some(value) => Poll::Ready(Some(value)),
                None => Poll::Pending,
            },
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}
