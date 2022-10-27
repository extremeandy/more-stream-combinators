use std::{
    task::{ready, Poll},
    time::{Duration, Instant},
};

use core::pin::Pin;
use futures::{
    channel::mpsc,
    stream::{self, BoxStream},
    SinkExt, Stream, StreamExt,
};
use more_stream_combinators::switch::StreamExt as MyStreamExt;
use pin_project_lite::pin_project;
use tokio_stream::{wrappers::IntervalStream, StreamExt as TokioStreamExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + 'static>> {
    // let (mut tx1, rx1) = mpsc::channel(100000);
    // //let (mut tx2, rx2) = mpsc::channel(16);

    // tokio::spawn(async move {
    //     for i in 1..1000 {
    //         tx1.send(i).await.unwrap();
    //         tokio::time::sleep(Duration::from_micros(10)).await;
    //     }
    // });

    let rx1 = IntervalStream::new(tokio::time::interval(Duration::from_micros(1000)));
    //let rx1 = stream::repeat(1).enumerate();

    // let (mut tx, rx) = mpsc::channel(16);

    // tokio::spawn(async move {
    //     tx.send(rx1).await.unwrap();
    //     tokio::time::sleep(Duration::from_millis(1500)).await;

    //     tx.send(rx2).await.unwrap();

    //     tokio::spawn(async move {
    //         for i in 1000..2000 {
    //             tx2.send(i).await;
    //             tokio::time::sleep(Duration::from_millis(250)).await;
    //         }
    //     });

    //     tokio::time::sleep(Duration::from_millis(1000)).await;
    //     drop(tx);
    // });

    //let mut combined = rx.switch();
    let sampler = IntervalStream::new(tokio::time::interval(Duration::from_millis(1)));
    let mut combined = rx1.sample(sampler);
    //let mut combined = rx.zip(stream::repeat(5));

    //println!("{combined:?}");

    // //rx1.chain(other)
    // //rx1.zip(other)

    // let mut combined = combine_latest(rx1, rx2).boxed();

    'outer: loop {
        let mut count = 0;
        let deadline = Instant::now() + Duration::from_secs(1);
        while let Some(t) = futures::StreamExt::next(&mut combined).await {
            if Instant::now() >= deadline {
                println!("count: {count}");
                continue 'outer;
            }

            count += 1;
            //println!("{t:?}");
        }
    }

    Ok(())
}

fn combine_latest<St1, St2>(stream1: St1, stream2: St2) -> CombineLatestStream<St1, St2>
where
    St1: Stream,
    St2: Stream,
{
    CombineLatestStream::new(stream1, stream2)
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
    struct CombineLatestStream<St1, St2>
        where
            St1: Stream,
            St2: Stream,
        {
            #[pin]
            first: Option<St1>,
            #[pin]
            second: Option<St2>,
            first_latest: Option<St1::Item>,
            second_latest: Option<St2::Item>,
        }
}

// All interactions with `Pin<&mut Chain<..>>` happen through these methods
impl<St1, St2> CombineLatestStream<St1, St2>
where
    St1: Stream,
    St2: Stream,
{
    pub fn new(stream1: St1, stream2: St2) -> Self {
        Self {
            first: Some(stream1),
            second: Some(stream2),
            first_latest: None,
            second_latest: None,
        }
    }
}

impl<St1, St2> Stream for CombineLatestStream<St1, St2>
where
    St1: Stream,
    St1::Item: Clone,
    St2: Stream<Item = St1::Item>,
    St2::Item: Clone,
{
    type Item = (St1::Item, St2::Item);

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let mut this = self.project();

        let mut at_least_one_ready_and_valid = false;

        if let Some(first) = this.first.as_mut().as_pin_mut() {
            if let Poll::Ready(item) = first.poll_next(cx) {
                match item {
                    Some(value) => {
                        *this.first_latest = Some(value);
                        at_least_one_ready_and_valid = true;
                    }
                    None => this.first.set(None),
                }
            }
        }

        if let Some(second) = this.second.as_mut().as_pin_mut() {
            if let Poll::Ready(item) = second.poll_next(cx) {
                match item {
                    Some(value) => {
                        *this.second_latest = Some(value);
                        at_least_one_ready_and_valid = true;
                    }
                    None => this.second.set(None),
                }
            }
        }

        // Cases:
        // 1. Both streams Pending -> we are pending
        // 2. Both streams Ready -> we are ready with either Some or None
        // 3. One stream ready / other pending
        // a) Ready(Some) -> do we have a value for 2nd stream? if yes, emit, otherwise pending.
        // b) Ready(None) -> pending, i think?

        // We are Pending when:
        // 1. First is pending AND second is pending
        // 2. First is Ready(None) and second is pending OR First is pending and Second is Ready(None)

        if this.first.is_none() && this.second.is_none() {
            // If both streams have ended, then the combined has ended.
            return std::task::Poll::Ready(None);
        } else if at_least_one_ready_and_valid {
            if let (Some(value1), Some(value2)) = (this.first_latest, this.second_latest) {
                return std::task::Poll::Ready(Some((value1.clone(), value2.clone())));
            }
        }

        return std::task::Poll::Pending;
    }
}
