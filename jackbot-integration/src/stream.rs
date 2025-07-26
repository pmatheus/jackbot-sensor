//! Stream utilities and implementations

use futures::Stream;
use pin_project::pin_project;
use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Exchange stream that processes data through a transformer
#[pin_project]
pub struct ExchangeStream<Parser, StreamType, OutputType, ErrorType> {
    #[pin]
    stream: StreamType,
    buffered_events: VecDeque<Result<OutputType, ErrorType>>,
    _parser: std::marker::PhantomData<Parser>,
}

impl<Parser, StreamType, OutputType, ErrorType> ExchangeStream<Parser, StreamType, OutputType, ErrorType> {
    pub fn new(
        stream: StreamType,
        buffered_events: VecDeque<Result<OutputType, ErrorType>>,
    ) -> Self {
        Self {
            stream,
            buffered_events,
            _parser: std::marker::PhantomData,
        }
    }
}

impl<Parser, StreamType, OutputType, ErrorType> Stream for ExchangeStream<Parser, StreamType, OutputType, ErrorType>
where
    StreamType: Stream + Unpin,
    OutputType: 'static,
    ErrorType: 'static,
{
    type Item = Result<OutputType, ErrorType>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // Use pin projection to safely access fields
        let this = self.project();
        
        // First, return any buffered events
        if let Some(event) = this.buffered_events.pop_front() {
            return Poll::Ready(Some(event));
        }

        // Then poll the underlying stream
        match this.stream.poll_next(cx) {
            Poll::Ready(Some(_message)) => {
                // For now, just continue polling since we don't have transformation logic
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}