//! Bounded raw transport sink. Physical shutdown wakes an abandoned consumer.
use std::{
    future::Future,
    io,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncWrite, DuplexStream};
pub(crate) struct ClosingWriter {
    writer: DuplexStream,
    stopped: Pin<Box<dyn Future<Output = ()> + Send>>,
}
impl ClosingWriter {
    pub fn new(writer: DuplexStream, stop: processkit::CancellationToken) -> Self {
        Self {
            writer,
            stopped: Box::pin(stop.cancelled_owned()),
        }
    }
}
impl AsyncWrite for ClosingWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        if self.stopped.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Err(io::ErrorKind::BrokenPipe.into()));
        }
        Pin::new(&mut self.writer).poll_write(cx, bytes)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if self.stopped.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Err(io::ErrorKind::BrokenPipe.into()));
        }
        Pin::new(&mut self.writer).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.writer).poll_shutdown(cx)
    }
}
