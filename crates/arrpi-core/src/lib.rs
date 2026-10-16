//! Shared media contracts and policy, independent of HTTP, MCP SDK and storage.

pub mod admission;
pub mod arguments;
mod commands;
pub mod config;
pub mod contracts;
pub mod error;
pub mod executor;
pub mod library;
pub mod oauth;
pub mod policy;
pub mod registry;
pub mod schema;
pub mod secret;
pub mod source;
pub mod subscriptions;
pub mod summaries;
pub mod tracking;

/// An immutable revision of source capabilities, tool policy and observation wiring.
#[derive(Debug)]
pub struct Commands {
    pub policy: policy::Policy,
    pub sources: source::Sources,
    registry: registry::Registry,
    events: Option<executor::Events>,
    normalizations: admission::Normalizations,
    clock: fn() -> tokio::time::Instant,
}
