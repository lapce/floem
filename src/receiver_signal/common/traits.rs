/// Error type indicating a channel has closed.
#[derive(Debug, Clone, Copy)]
pub struct ChannelClosed;

impl std::fmt::Display for ChannelClosed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Channel Closed")
    }
}

impl core::error::Error for ChannelClosed {}

/// Trait for receivers that can be polled (not blocking).
pub trait PollableReceiver {
    type Item;
    type Error;

    fn poll_recv(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<Option<Self::Item>, Self::Error>>;
}

/// Trait for blocking receivers.
pub trait BlockingReceiver {
    type Item;
    type Error;

    fn recv(&mut self) -> Result<Self::Item, Self::Error>;
}

impl<T> BlockingReceiver for std::sync::mpsc::Receiver<T> {
    type Item = T;
    type Error = std::sync::mpsc::RecvError;

    fn recv(&mut self) -> Result<Self::Item, Self::Error> {
        std::sync::mpsc::Receiver::recv(self)
    }
}

#[cfg(feature = "crossbeam")]
impl<T> BlockingReceiver for crossbeam::channel::Receiver<T> {
    type Item = T;
    type Error = crossbeam::channel::RecvError;

    fn recv(&mut self) -> Result<Self::Item, Self::Error> {
        crossbeam::channel::Receiver::recv(self)
    }
}

// Implement PollableReceiver for tokio channels

#[cfg(feature = "tokio")]
impl<T> PollableReceiver for tokio::sync::mpsc::Receiver<T> {
    type Item = T;
    type Error = ChannelClosed;

    fn poll_recv(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<Option<Self::Item>, Self::Error>> {
        use std::future::Future;
        let recv_fut = self.recv();
        let mut recv_fut = std::pin::pin!(recv_fut);

        match recv_fut.as_mut().poll(cx) {
            std::task::Poll::Pending => std::task::Poll::Pending,
            std::task::Poll::Ready(Some(v)) => std::task::Poll::Ready(Ok(Some(v))),
            std::task::Poll::Ready(None) => std::task::Poll::Ready(Err(ChannelClosed)),
        }
    }
}

#[cfg(feature = "tokio")]
impl<T> PollableReceiver for tokio::sync::mpsc::UnboundedReceiver<T> {
    type Item = T;
    type Error = ChannelClosed;

    fn poll_recv(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<Option<Self::Item>, Self::Error>> {
        use std::future::Future;
        let recv_fut = self.recv();
        let mut recv_fut = std::pin::pin!(recv_fut);

        match recv_fut.as_mut().poll(cx) {
            std::task::Poll::Pending => std::task::Poll::Pending,
            std::task::Poll::Ready(Some(v)) => std::task::Poll::Ready(Ok(Some(v))),
            std::task::Poll::Ready(None) => std::task::Poll::Ready(Err(ChannelClosed)),
        }
    }
}

#[cfg(feature = "tokio")]
impl<T: Clone> PollableReceiver for tokio::sync::broadcast::Receiver<T> {
    type Item = T;
    type Error = tokio::sync::broadcast::error::RecvError;

    fn poll_recv(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<Option<Self::Item>, Self::Error>> {
        use std::future::Future;
        let recv_fut = self.recv();
        let mut recv_fut = std::pin::pin!(recv_fut);

        match recv_fut.as_mut().poll(cx) {
            std::task::Poll::Pending => std::task::Poll::Pending,
            std::task::Poll::Ready(Ok(v)) => std::task::Poll::Ready(Ok(Some(v))),
            std::task::Poll::Ready(Err(e)) => std::task::Poll::Ready(Err(e)),
        }
    }
}
