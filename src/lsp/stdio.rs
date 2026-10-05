//! Client input for the stdio transport, observed so the process can end once
//! its client is gone.
//!
//! `tower_lsp::Server::serve` returns only after every in-flight handler
//! completes. A handler awaiting a response from a client that closed its input,
//! such as `client/registerCapability` sent from `initialized`, never completes,
//! so the server must not rely on `serve` returning to exit.

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use tokio::io::{AsyncRead, ReadBuf};
use tokio::sync::oneshot;

/// How long handlers that can still finish get to flush their responses after
/// the client input closes, before the process exits regardless.
pub const CLOSED_INPUT_DRAIN: Duration = Duration::from_secs(2);

/// Wrap the client input and return the signal raised when it reaches EOF or fails.
pub fn client_input<R>(inner: R) -> (ClientInput<R>, InputClosed) {
    let (closed, receiver) = oneshot::channel();
    (
        ClientInput {
            inner,
            closed: Some(closed),
        },
        InputClosed { receiver },
    )
}

pub struct ClientInput<R> {
    inner: R,
    closed: Option<oneshot::Sender<()>>,
}

impl<R> ClientInput<R> {
    fn mark_closed(&mut self) {
        if let Some(closed) = self.closed.take() {
            let _ = closed.send(());
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for ClientInput<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let filled = buffer.filled().len();
        let poll = Pin::new(&mut this.inner).poll_read(context, buffer);
        match &poll {
            Poll::Ready(Ok(())) if buffer.remaining() > 0 && buffer.filled().len() == filled => {
                this.mark_closed();
            }
            Poll::Ready(Err(_)) => this.mark_closed(),
            Poll::Ready(Ok(())) | Poll::Pending => {}
        }
        poll
    }
}

pub struct InputClosed {
    receiver: oneshot::Receiver<()>,
}

impl InputClosed {
    /// Resolve once the client input has closed; never resolves if the input
    /// was dropped without closing, because then the transport ended on its own.
    pub async fn closed(self) {
        if self.receiver.await.is_err() {
            std::future::pending::<()>().await;
        }
    }
}
