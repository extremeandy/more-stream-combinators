use std::{
    task::{ready, Poll},
    time::Duration,
};

use core::pin::Pin;
use futures::{
    channel::mpsc,
    future::BoxFuture,
    stream::{futures_unordered::IntoIter, BoxStream, FuturesOrdered, FuturesUnordered, Next},
    Future, SinkExt, Stream, StreamExt,
};
use pin_project_lite::pin_project;

pub fn combine_latest<St: Stream, F: Future<Output = St::Item>>(
    streams: Vec<St>,
) -> CombineLatestStream<St, F> {
    CombineLatestStream::new(streams)
}
// let mut identifier_one = identifier_one.boxed();
// let mut identifier_two = identifier_two.boxed();

// let mut a = None;
// let mut b = None;

// async_stream::stream! {
//     loop {
//         tokio::select! {
//             Some(next_a) = identifier_one.next() => { a = next_a.into(); }
//             Some(next_b) = identifier_two.next() => { b = next_b.into(); }
//             else => break,
//         }
//         if let (Some(a), Some(b)) = (&a, &b) {
//             yield (a, b)
//         }
//     }
// }

pin_project! {
    #[derive(Debug)]
    #[must_use = "streams do nothing unless polled"]
    struct CombineLatestStream<St: Stream, F: Future<Output = St::Item>>
        {
            #[pin]
            streams: Vec<St>,
            futures_unordered: FuturesUnordered<F>
            //first_latest: Option<St1::Item>,
            //second_latest: Option<St2::Item>,
        }
}

// All interactions with `Pin<&mut Chain<..>>` happen through these methods
impl<St: Stream + Send, F: Future<Output = St::Item>> CombineLatestStream<St, F> {
    pub fn new(streams: Vec<St>) -> Self {
        let fu = FuturesUnordered::new();

        let streams: Vec<_> = streams.into_iter().map(|s| s.boxed()).collect();

        for stream in streams.iter_mut() {
            let fut = stream.next();
            fu.push(fut);
        }

        Self {
            streams: streams,
            futures_unordered: fu,
        }
    }
}

// impl<St, F: Future<Output = St::Item>> Stream for CombineLatestStream<St, F>
// where
//     St: Stream,
//     St::Item: Clone,
// {
//     type Item = Vec<St::Item>;

//     fn poll_next(
//         self: std::pin::Pin<&mut Self>,
//         cx: &mut std::task::Context<'_>,
//     ) -> std::task::Poll<Option<Self::Item>> {
//         let mut this = self.project();

//         let mut at_least_one_ready_and_valid = false;

//         if let Some(first) = this.first.as_mut().as_pin_mut() {
//             if let Poll::Ready(item) = first.poll_next(cx) {
//                 match item {
//                     Some(value) => {
//                         *this.first_latest = Some(value);
//                         at_least_one_ready_and_valid = true;
//                     }
//                     None => this.first.set(None),
//                 }
//             }
//         }

//         if let Some(second) = this.second.as_mut().as_pin_mut() {
//             if let Poll::Ready(item) = second.poll_next(cx) {
//                 match item {
//                     Some(value) => {
//                         *this.second_latest = Some(value);
//                         at_least_one_ready_and_valid = true;
//                     }
//                     None => this.second.set(None),
//                 }
//             }
//         }

//         // Cases:
//         // 1. Both streams Pending -> we are pending
//         // 2. Both streams Ready -> we are ready with either Some or None
//         // 3. One stream ready / other pending
//         // a) Ready(Some) -> do we have a value for 2nd stream? if yes, emit, otherwise pending.
//         // b) Ready(None) -> pending, i think?

//         // We are Pending when:
//         // 1. First is pending AND second is pending
//         // 2. First is Ready(None) and second is pending OR First is pending and Second is Ready(None)

//         if this.first.is_none() && this.second.is_none() {
//             // If both streams have ended, then the combined has ended.
//             return std::task::Poll::Ready(None);
//         } else if at_least_one_ready_and_valid {
//             if let (Some(value1), Some(value2)) = (this.first_latest, this.second_latest) {
//                 return std::task::Poll::Ready(Some((value1.clone(), value2.clone())));
//             }
//         }

//         return std::task::Poll::Pending;
//     }
// }
