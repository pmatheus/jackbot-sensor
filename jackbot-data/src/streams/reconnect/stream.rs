use crate::streams::{consumer::StreamKey, reconnect::Event};
use chrono::Utc;
use derive_more::Constructor;
use futures::Stream;
use futures_util::StreamExt;
use jackbot_integration::{
    metric::{Field, Metric, Tag},
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, convert, fmt::Debug, future, future::Future};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// Utilities for handling a continually reconnecting [`Stream`] initialised via the
/// [`init_reconnecting_stream`] function.
pub trait ReconnectingStream
where
    Self: Stream + Sized,
{
    /// Add an exponential backoff policy to an initialised [`ReconnectingStream`] using the
    /// provided [`ReconnectionBackoffPolicy`].
    #[allow(clippy::cognitive_complexity)]
    fn with_reconnect_backoff<St, InitError>(
        self,
        policy: ReconnectionBackoffPolicy,
        stream_key: StreamKey,
    ) -> impl Stream<Item = St>
    where
        Self: Stream<Item = Result<St, InitError>>,
        St: Stream,
        InitError: Debug,
    {
        self.enumerate()
            .scan(
                ReconnectionState::from(policy),
                move |state, (attempt, result)| match result {
                    Ok(stream) => {
                        info!(attempt, ?stream_key, "successfully initialised Stream");
                        let mut fields = HashMap::new();
                        fields.insert("attempt".to_string(), Field::from(attempt as u64));
                        let metric = Metric {
                            name: "ws_reconnect_success".to_string(),
                            timestamp: Utc::now().timestamp_millis(),
                            fields,
                            tags: vec![
                                Tag::new("exchange", stream_key.exchange.as_str()),
                                Tag::new("stream", stream_key.stream),
                            ],
                        };
                        info!(?metric, "WebSocket reconnected successfully");
                        state.reset_backoff();
                        futures::future::Either::Left(future::ready(Some(Ok(stream))))
                    }
                    Err(error) => {
                        warn!(
                            attempt,
                            ?stream_key,
                            ?error,
                            "failed to re-initialise Stream"
                        );
                        let sleep_duration = state.generate_sleep_duration();
                        let mut fields = HashMap::new();
                        fields.insert("attempt".to_string(), Field::from(attempt as u64));
                        fields.insert("backoff_ms".to_string(), Field::from(sleep_duration.as_millis() as u64));
                        let metric = Metric {
                            name: "ws_reconnect_backoff".to_string(),
                            timestamp: Utc::now().timestamp_millis(),
                            fields,
                            tags: vec![
                                Tag::new("exchange", stream_key.exchange.as_str()),
                                Tag::new("stream", stream_key.stream),
                            ],
                        };
                        warn!(?metric, "waiting before reconnect attempt");
                        let sleep_fut = tokio::time::sleep(sleep_duration);
                        state.multiply_backoff();
                        futures::future::Either::Right(Box::pin(async move {
                            sleep_fut.await;
                            Some(Err(error))
                        }))
                    }
                },
            )
            .filter_map(|result| future::ready(result.ok()))
    }

    /// Terminates the inner [`Stream`] if the encountered error is determined to be unrecoverable
    /// by the provided closure. This will cause the [`ReconnectingStream`] to re-initialise the
    /// inner [`Stream`].
    fn with_termination_on_error<St, T, E, FnIsTerminal>(
        self,
        is_terminal: FnIsTerminal,
        stream_key: StreamKey,
    ) -> impl Stream<Item = impl Stream<Item = Result<T, E>>>
    where
        Self: Stream<Item = St>,
        St: Stream<Item = Result<T, E>>,
        FnIsTerminal: Fn(&E) -> bool + Copy,
    {
        self.map(move |stream| {
            tokio_stream::StreamExt::map_while(stream, {
                move |result| match result {
                    Ok(item) => Some(Ok(item)),
                    Err(error) if is_terminal(&error) => {
                        error!(
                            ?stream_key,
                            "MarketStream encountered terminal error that requires reconnecting"
                        );
                        None
                    }
                    Err(error) => Some(Err(error)),
                }
            })
        })
    }

    /// Maps every [`ReconnectingStream`] `Stream::Item` into an [`reconnect::Event::Item`](Event),
    /// and chain a [`reconnect::Event::Reconnecting`](Event)
    fn with_reconnection_events<St, Origin>(
        self,
        origin: Origin,
    ) -> impl Stream<Item = Event<Origin, St::Item>>
    where
        Self: Stream<Item = St>,
        St: Stream,
        Origin: Clone + 'static,
    {
        self.map(move |stream| {
            stream
                .map(Event::Item)
                .chain(futures::stream::once(future::ready(Event::Reconnecting(
                    origin.clone(),
                ))))
        })
        .flatten()
    }

    /// Handles all encountered errors with the provided closure before filtering them out,
    /// returning a [`Stream`] of the Ok values. Useful for logging recoverable errors before
    /// continuing.
    fn with_error_handler<FnOnErr, Origin, T, E>(
        self,
        op: FnOnErr,
    ) -> impl Stream<Item = Event<Origin, T>>
    where
        Self: Stream<Item = Event<Origin, Result<T, E>>>,
        FnOnErr: Fn(E) + 'static,
    {
        self.filter_map(move |event| {
            std::future::ready(match event {
                Event::Reconnecting(origin) => Some(Event::Reconnecting(origin)),
                Event::Item(Ok(item)) => Some(Event::Item(item)),
                Event::Item(Err(error)) => {
                    op(error);
                    None
                }
            })
        })
    }

    /// Future for forwarding items in [`Self`] to the provided channel [`Tx`].
    fn forward_to<T>(self, tx: mpsc::UnboundedSender<T>) -> impl Future<Output = ()> + Send
    where
        Self: Stream + Sized + Send,
        Self::Item: Into<T>,
        T: Send + 'static,
    {
        async move {
            tokio_stream::StreamExt::map_while(self, move |event| {
                tx.send(event.into()).ok().map(|_| ())
            }).collect::<()>().await
        }
    }
}

impl<T> ReconnectingStream for T where T: Stream {}

/// Initialise a [`ReconnectingStream`] using the provided initialisation closure.
pub async fn init_reconnecting_stream<FnInit, St, FnInitError, FnInitFut>(
    init_stream: FnInit,
) -> Result<impl Stream<Item = Result<St, FnInitError>>, FnInitError>
where
    FnInit: Fn() -> FnInitFut,
    FnInitFut: Future<Output = Result<St, FnInitError>>,
{
    let initial = init_stream().await?;
    let reconnections = futures::stream::repeat_with(init_stream).then(convert::identity);

    Ok(futures::stream::once(future::ready(Ok(initial))).chain(reconnections))
}

/// Reconnection backoff policy for a [`ReconnectingStream::with_reconnect_backoff`].
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize, Constructor,
)]
pub struct ReconnectionBackoffPolicy {
    /// Initial backoff millisecond duration after the first `Stream` disconnection.
    ///
    /// This value then scales with the `backoff_multiplier` in the case of repeated failed
    /// `Stream` reconnection attempts.
    pub backoff_ms_initial: u64,

    /// Scaling factor for the backoff duration in the case of repeated `Stream` reconnection
    /// attempts.
    pub backoff_multiplier: u8,

    /// Maximum possible backoff duration between reconnection attempts.
    pub backoff_ms_max: u64,

    /// Random jitter in milliseconds to apply on top of the calculated backoff
    /// duration. A random value in the range `[0, jitter_ms]` will be added to
    /// each reconnection delay.
    pub jitter_ms: u64,
}

// Note: ReconnectionState has been moved to the test file for now.
// Consider making it public if it needs to be accessed from outside this module
// or refactoring tests to not depend on its internal structure.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
struct ReconnectionState {
    policy: ReconnectionBackoffPolicy,
    backoff_ms_current: u64,
}

impl From<ReconnectionBackoffPolicy> for ReconnectionState {
    fn from(policy: ReconnectionBackoffPolicy) -> Self {
        Self {
            backoff_ms_current: policy.backoff_ms_initial,
            policy,
        }
    }
}

impl ReconnectionState {
    fn reset_backoff(&mut self) {
        self.backoff_ms_current = self.policy.backoff_ms_initial;
    }

    fn multiply_backoff(&mut self) {
        let next = self.backoff_ms_current * self.policy.backoff_multiplier as u64;
        let next_capped = std::cmp::min(next, self.policy.backoff_ms_max);
        self.backoff_ms_current = next_capped;
    }

    fn generate_sleep_duration(&self) -> std::time::Duration {
        let jitter = if self.policy.jitter_ms > 0 {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            rng.gen_range(0..=self.policy.jitter_ms)
        } else {
            0
        };

        std::time::Duration::from_millis(self.backoff_ms_current + jitter)
    }
}
