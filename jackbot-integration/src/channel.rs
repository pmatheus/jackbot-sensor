//! Channel utilities for async communication

use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

/// Channel trait for generic communication
pub trait Channel<T> {
    type Sender;
    type Receiver;
    type Error;
    
    fn send(&self, item: T) -> Result<(), Self::Error>;
    fn try_recv(&mut self) -> Result<T, Self::Error>;
}

/// Unbounded sender wrapper
pub type UnboundedTx<T> = mpsc::UnboundedSender<T>;

/// Unbounded receiver wrapper
pub type UnboundedRx<T> = mpsc::UnboundedReceiver<T>;

/// General sender wrapper
pub type Tx<T> = mpsc::Sender<T>;

/// General receiver wrapper  
pub type Rx<T> = mpsc::Receiver<T>;

/// Create an unbounded channel
pub fn mpsc_unbounded<T>() -> (UnboundedTx<T>, UnboundedRx<T>) {
    mpsc::unbounded_channel()
}

/// Bounded sender wrapper
pub type BoundedTx<T> = mpsc::Sender<T>;

/// Bounded receiver wrapper  
pub type BoundedRx<T> = mpsc::Receiver<T>;

/// Create a bounded channel
pub fn mpsc_bounded<T>(buffer: usize) -> (BoundedTx<T>, BoundedRx<T>) {
    mpsc::channel(buffer)
}

/// Generic channel implementation for unbounded channels
#[derive(Debug)]
pub struct UnboundedChannel<T> {
    tx: UnboundedTx<T>,
    rx: UnboundedRx<T>,
}

impl<T> UnboundedChannel<T> {
    pub fn new() -> Self {
        let (tx, rx) = mpsc_unbounded();
        Self { tx, rx }
    }
    
    pub fn sender(&self) -> UnboundedTx<T> {
        self.tx.clone()
    }
    
    pub fn split(self) -> (UnboundedTx<T>, UnboundedRx<T>) {
        (self.tx, self.rx)
    }
}

impl<T> Default for UnboundedChannel<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Channel<T> for UnboundedTx<T> {
    type Sender = Self;
    type Receiver = UnboundedRx<T>;
    type Error = mpsc::error::SendError<T>;
    
    fn send(&self, item: T) -> Result<(), Self::Error> {
        self.send(item)
    }
    
    fn try_recv(&mut self) -> Result<T, Self::Error> {
        // This doesn't make sense for a sender, but we need to implement the trait
        Err(mpsc::error::SendError(unsafe { std::mem::zeroed() }))
    }
}

/// Extension trait for converting UnboundedReceiver to Stream
pub trait UnboundedReceiverExt<T> {
    /// Convert the receiver into a stream
    fn into_stream(self) -> UnboundedReceiverStream<T>;
}

impl<T> UnboundedReceiverExt<T> for UnboundedRx<T> {
    fn into_stream(self) -> UnboundedReceiverStream<T> {
        UnboundedReceiverStream::new(self)
    }
}