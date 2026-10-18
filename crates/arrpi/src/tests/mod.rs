//! Local mock upstreams and regressions for HTTP, cache and CLI contracts.

mod calendar;
mod cli;
mod commands;
mod config;
mod library;
mod maintenance;
mod oauth;
mod proxy;
mod rss;
mod settings;
mod subscriptions;
mod summaries;
mod tracking;
mod transport;

use std::sync::{
    Arc,
    atomic::{AtomicU16, AtomicU64, AtomicUsize, Ordering},
};

use axum::{
    Router,
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    response::Response,
    routing::get,
};
use tokio::{sync::Mutex, task::JoinHandle};

use crate::{config::Config, proxy::Gateway};

const CLIENT_KEY: &str = "fake-client-key-00000000000000000000";
const OTHER_KEY: &str = "fake-client-key-11111111111111111111";
const UPSTREAM_KEY: &str = "fake-upstream-api-key";
const EXAMPLE: &str = include_str!("../../../../tests/settings.example.json");

#[derive(Debug)]
struct Mock {
    calls: AtomicUsize,
    status: AtomicU16,
    delay_millis: AtomicU64,
    entered: tokio::sync::Notify,
    body: Mutex<String>,
    seen: Mutex<Vec<(String, HeaderMap)>>,
}

#[derive(Debug)]
struct Server {
    url: String,
    task: JoinHandle<()>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn server(router: Router) -> Server {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind local fixture listener");
    let address = listener
        .local_addr()
        .expect("fixture listener has an address");
    let task = tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("serve local fixture");
    });

    Server {
        url: format!("http://{address}/"),
        task,
    }
}

async fn mock(body: &str) -> (Server, Arc<Mock>) {
    let state = Arc::new(Mock {
        calls: AtomicUsize::new(0),
        status: AtomicU16::new(200),
        delay_millis: AtomicU64::new(0),
        entered: tokio::sync::Notify::new(),
        body: Mutex::new(body.into()),
        seen: Mutex::new(Vec::with_capacity(32)),
    });
    let router = Router::new()
        .route("/{*path}", get(reply))
        .with_state(Arc::clone(&state));

    (server(router).await, state)
}

async fn reply(State(mock): State<Arc<Mock>>, request: Request) -> Response {
    mock.calls.fetch_add(1, Ordering::Relaxed);
    mock.entered.notify_one();
    tokio::time::sleep(std::time::Duration::from_millis(
        mock.delay_millis.load(Ordering::Relaxed),
    ))
    .await;
    mock.seen
        .lock()
        .await
        .push((request.uri().to_string(), request.headers().clone()));

    let status =
        StatusCode::from_u16(mock.status.load(Ordering::Relaxed)).expect("valid fixture status");
    let mut response = Response::new(Body::from(mock.body.lock().await.clone()));

    *response.status_mut() = status;
    response.headers_mut().insert(
        "content-type",
        "application/json".parse().expect("literal header"),
    );
    response.headers_mut().insert(
        "set-cookie",
        "native-session=must-not-leak"
            .parse()
            .expect("literal header"),
    );
    response.headers_mut().insert(
        "location",
        "http://127.0.0.1:9/redirect"
            .parse()
            .expect("literal header"),
    );

    response
}

fn gateway(url: &str) -> Gateway {
    let text = EXAMPLE
        .replace("http://sonarr:8989/", url)
        .replace("http://radarr:7878/", url);
    let config = Config::parse(&text).expect("valid example config");

    Gateway::new(&config, tokio::time::Instant::now).expect("valid fixture gateway")
}

fn resolve(name: &str) -> Option<String> {
    match name {
        "SONARR_API_KEY" | "RADARR_API_KEY" => Some(UPSTREAM_KEY.into()),
        "ARRPI_READER_KEY" => Some(CLIENT_KEY.into()),
        "ARRPI_CALENDAR_KEY" => Some(OTHER_KEY.into()),
        _ => None,
    }
}
