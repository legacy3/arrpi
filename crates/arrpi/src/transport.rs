//! Connection and response leases that survive handler completion.

use std::{
    io,
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use axum::{body::Body, serve::Listener};
use http_body::{Frame, SizeHint};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpListener, TcpStream},
    sync::{OwnedSemaphorePermit, Semaphore},
    time::{Instant, Sleep},
};

use crate::config::CachePolicy;

#[derive(Debug)]
pub(crate) struct LimitedListener {
    inner: TcpListener,
    slots: Arc<Semaphore>,
    idle: Duration,
    lifetime: Duration,
    clock: fn() -> Instant,
}

#[derive(Debug)]
pub(crate) struct Connection {
    inner: TcpStream,
    _permit: OwnedSemaphorePermit,
    deadline: Pin<Box<Sleep>>,
    expires: Instant,
    idle: Duration,
    clock: fn() -> Instant,
}

#[derive(Debug)]
pub(crate) struct LeasedBody {
    pub(crate) inner: Body,
    pub(crate) _permit: OwnedSemaphorePermit,
}

impl LimitedListener {
    pub(crate) fn new(inner: TcpListener, policy: CachePolicy, clock: fn() -> Instant) -> Self {
        Self {
            inner,
            slots: Arc::new(Semaphore::new(policy.max_connections)),
            idle: Duration::from_secs(policy.connection_idle_seconds),
            lifetime: Duration::from_secs(policy.connection_lifetime_seconds),
            clock,
        }
    }
}

impl Listener for LimitedListener {
    type Io = Connection;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let Ok(permit) = Arc::clone(&self.slots).acquire_owned().await else {
                std::future::pending::<()>().await;

                continue;
            };

            if let Ok((inner, address)) = self.inner.accept().await {
                let now = (self.clock)();
                let expires = now + self.lifetime;

                return (
                    Connection {
                        inner,
                        _permit: permit,
                        deadline: Box::pin(tokio::time::sleep_until(
                            (now + self.idle).min(expires),
                        )),
                        expires,
                        idle: self.idle,
                        clock: self.clock,
                    },
                    address,
                );
            }

            drop(permit);
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.inner.local_addr()
    }
}

impl Connection {
    fn expired(&mut self, cx: &mut Context<'_>) -> bool {
        self.deadline.as_mut().poll(cx).is_ready()
    }

    fn progress(&mut self) {
        self.deadline
            .as_mut()
            .reset(((self.clock)() + self.idle).min(self.expires));
    }
}

impl AsyncRead for Connection {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.expired(cx) {
            return Poll::Ready(Err(io::ErrorKind::TimedOut.into()));
        }

        let before = buf.filled().len();
        let result = Pin::new(&mut self.inner).poll_read(cx, buf);

        if matches!(result, Poll::Ready(Ok(()))) && buf.filled().len() > before {
            self.progress();
        }

        result
    }
}

impl AsyncWrite for Connection {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if self.expired(cx) {
            return Poll::Ready(Err(io::ErrorKind::TimedOut.into()));
        }

        let result = Pin::new(&mut self.inner).poll_write(cx, buf);

        if matches!(result, Poll::Ready(Ok(bytes)) if bytes > 0) {
            self.progress();
        }

        result
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if self.expired(cx) {
            return Poll::Ready(Err(io::ErrorKind::TimedOut.into()));
        }

        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

type BodyFrame = Result<Frame<bytes::Bytes>, axum::Error>;

impl http_body::Body for LeasedBody {
    type Data = bytes::Bytes;
    type Error = axum::Error;

    fn poll_frame(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<BodyFrame>> {
        Pin::new(&mut self.inner).poll_frame(cx)
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}
