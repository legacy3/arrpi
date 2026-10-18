//! Shared command policy and use cases, independent of protocol and concrete providers.

mod audit;
mod batch;
mod broker;
mod calendar;
mod library;
mod maintenance;
mod permissions;
mod profiles;
mod services;
mod summaries;

use crate::{
    Commands, arguments as args,
    config::Config,
    contracts::{Calendar, McpStatus, ToolVisibility},
    error::ApiError,
    executor::Recording,
    registry::{Registry, ToolDefinition, ToolId},
    source::{Actor, ApiSource, Principal, Sources},
};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Caller<'a> {
    principal: &'a Principal,
    actor: Option<Actor>,
    compatible: bool,
}

impl Commands {
    /// Construct a complete immutable runtime, including its optional observation sink.
    #[must_use]
    pub fn new(
        config: &Config,
        sources: Sources,
        recording: Recording,
        normalizations: crate::admission::Normalizations,
    ) -> Self {
        Self {
            policy: config.mcp.clone(),
            sources,
            registry: Registry::new(config.mcp.limits),
            events: recording.events,
            normalizations,
            clock: recording.clock,
        }
    }

    /// Shared tools require a configured account broker and the global enable switch.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.policy.enabled && self.sources.account_source().is_some()
    }

    /// Read an already built descriptor without allocating another schema.
    #[must_use]
    pub fn definition(&self, tool: ToolId) -> &ToolDefinition {
        self.registry.definition(tool)
    }

    /// Discover currently permitted tools with fresh native account verification.
    ///
    /// # Errors
    /// Returns disabled transport. Unverified account tools are omitted.
    pub async fn list(&self, principal: &Principal) -> Result<Vec<ToolDefinition>, ApiError> {
        if !self.ready() {
            return Err(ApiError::NOT_FOUND);
        }

        let actor = self.actor(principal).await.unwrap_or(None);
        let compatible = actor.is_some() && self.compatible(principal).await.unwrap_or(false);
        let caller = Caller {
            principal,
            actor,
            compatible,
        };

        Ok(ToolId::ALL
            .iter()
            .filter(|tool| self.allowed(&caller, **tool))
            .map(|tool| self.definition(*tool).clone())
            .collect())
    }

    /// Execute one known command as the server-resolved principal.
    ///
    /// # Errors
    /// Preserves local policy failures, native rejections and indeterminate mutations.
    pub async fn execute(
        &self,
        principal: &Principal,
        tool: ToolId,
        mut input: Value,
    ) -> Result<Value, ApiError> {
        let started = (self.clock)();
        let audit_input = tool
            .is_write()
            .then(|| audit::capture(&input, &self.sources));

        self.default_page(tool, &mut input);
        let result = self.run(principal, tool, input).await;
        let elapsed = u64::try_from(
            (self.clock)()
                .saturating_duration_since(started)
                .as_millis(),
        )
        .unwrap_or(u64::MAX);

        if let Some(events) = &self.events {
            if let Some(input) = audit_input {
                let observations = audit::events(tool, &input, &result);

                if events
                    .audit(
                        &principal.name,
                        principal.binding.as_ref().map(|binding| binding.id),
                        &observations,
                        self.policy.limits.audit_max_entries,
                    )
                    .await
                    .is_err()
                {
                    tracing::warn!(event = "write_audit_unavailable");
                }
            }

            if events
                .statistics(&principal.name, tool, outcome(&result), elapsed)
                .await
                .is_err()
            {
                tracing::warn!(event = "tool_statistics_unavailable");
            }
        }

        result
    }

    /// Inspect capabilities independently from master/tool enable switches.
    ///
    /// # Errors
    /// Serialization failure is internal; native account failure stays in the status.
    pub async fn inspect(&self, principal: &Principal) -> Result<McpStatus, ApiError> {
        let resolved = self.actor(principal).await;
        let verified = self.compatible(principal).await;
        let connection_error = resolved
            .as_ref()
            .err()
            .or_else(|| verified.as_ref().err())
            .map(|error| error.code().to_owned());
        let caller = Caller {
            principal,
            actor: resolved.unwrap_or(None),
            compatible: verified.unwrap_or(false),
        };
        let tools = ToolId::ALL
            .iter()
            .map(|tool| {
                let definition = self.definition(*tool);

                ToolVisibility {
                    name: tool.name().to_owned(),
                    description: definition.description.to_string(),
                    available: (!tool.is_write() || tool.is_native() || caller.compatible)
                        && self.permitted(&caller, *tool),
                }
            })
            .collect();

        Ok(McpStatus {
            configured: self.sources.account_source().is_some(),
            connected: caller.actor.is_some(),
            writes_supported: caller.compatible,
            connection_error,
            tools,
        })
    }

    /// Read the calendar directly for an authenticated principal, independent of MCP.
    ///
    /// # Errors
    /// Rejects invalid windows, absent grants or exhausted normalization admission.
    pub async fn calendar(
        &self,
        principal: &Principal,
        args: args::Dates,
    ) -> Result<Calendar, ApiError> {
        let allowed = self
            .sources
            .iter()
            .filter(|(name, source)| {
                source.calendar().is_some() && principal.allows(name, "calendar-view")
            })
            .map(|(name, _)| name.to_owned())
            .collect::<Vec<_>>();

        self.calendar_for(&allowed, args).await
    }

    /// Read a calendar for already verified subscription source grants.
    ///
    /// # Errors
    /// Rejects empty/invalid grants, invalid windows and admission exhaustion.
    pub async fn calendar_for(
        &self,
        allowed: &[String],
        args: args::Dates,
    ) -> Result<Calendar, ApiError> {
        calendar::load(&self.sources, allowed, args).await
    }

    async fn actor(&self, principal: &Principal) -> Result<Option<Actor>, ApiError> {
        match (
            self.sources.account_source().and_then(ApiSource::accounts),
            principal.binding.as_ref(),
        ) {
            (Some(accounts), Some(binding)) => accounts.identity(binding).await.map(Some),
            _ => Ok(None),
        }
    }

    async fn compatible(&self, principal: &Principal) -> Result<bool, ApiError> {
        match (
            self.sources.account_source().and_then(ApiSource::accounts),
            principal.binding.as_ref(),
        ) {
            (Some(accounts), Some(binding)) => accounts.compatible(binding).await,
            _ => Ok(false),
        }
    }

    async fn run(
        &self,
        principal: &Principal,
        tool: ToolId,
        input: Value,
    ) -> Result<Value, ApiError> {
        if !self.ready() {
            return Err(ApiError::NOT_FOUND);
        }

        if self
            .policy
            .disabled_tools
            .iter()
            .any(|name| name.as_ref() == tool.name())
        {
            return Err(ApiError::FORBIDDEN);
        }

        validate_page(&input, self.policy.limits.max_page_items)?;
        let caller = self.authorize(principal, tool).await?;

        match tool {
            ToolId::RefreshMedia
            | ToolId::SearchReleases
            | ToolId::SeriesEpisodes
            | ToolId::MediaCommandStatus => self.maintenance(principal, tool, input).await,

            ToolId::ListMovies | ToolId::ListSeries | ToolId::SearchLibrary => {
                library::load(
                    &self.sources,
                    principal,
                    tool,
                    args::decode(input)?,
                    &self.normalizations,
                )
                .await
            }

            ToolId::QueueSummary | ToolId::LibrarySummary | ToolId::MonitoringStatus => {
                let kind = summary_kind(tool).ok_or(ApiError::NOT_FOUND)?;

                Ok(serde_json::to_value(
                    self.summaries(principal, kind, args::decode(input)?)
                        .await?,
                )?)
            }

            ToolId::ServiceStatus => {
                services::status(&self.sources, principal, args::decode(input)?).await
            }

            ToolId::UpcomingReleases => Ok(serde_json::to_value(
                self.calendar(principal, args::decode(input)?).await?,
            )?),

            _ => self.broker_command(&caller, tool, input).await,
        }
    }

    async fn authorize<'a>(
        &self,
        principal: &'a Principal,
        tool: ToolId,
    ) -> Result<Caller<'a>, ApiError> {
        if !self.locally_permitted(principal, tool) {
            return Err(ApiError::FORBIDDEN);
        }

        let actor = if tool.is_native() {
            None
        } else {
            self.actor(principal).await?
        };

        let mut caller = Caller {
            principal,
            actor,
            compatible: false,
        };

        if !self.permitted(&caller, tool) {
            return Err(ApiError::FORBIDDEN);
        }

        if tool.is_write() && !tool.is_native() {
            caller.compatible = self.compatible(principal).await?;

            if !caller.compatible {
                return Err(ApiError::UNSUPPORTED_VERSION);
            }
        }

        Ok(caller)
    }

    fn default_page(&self, tool: ToolId, input: &mut Value) {
        if tool.is_paginated() && input.is_object() && input.get("limit").is_none() {
            input["limit"] = json!(20.min(self.policy.limits.max_page_items));
        }
    }
}

fn validate_page(input: &Value, maximum: usize) -> Result<(), ApiError> {
    if input
        .get("limit")
        .and_then(Value::as_u64)
        .is_some_and(|limit| limit > u64::try_from(maximum).unwrap_or(u64::MAX))
    {
        return Err(ApiError::BAD_QUERY);
    }

    Ok(())
}

fn outcome(result: &Result<Value, ApiError>) -> &str {
    match result {
        Ok(value)
            if value
                .get("failed")
                .and_then(Value::as_u64)
                .is_some_and(|failed| failed > 0) =>
        {
            "partial_failure"
        }

        Ok(_) => "ok",

        Err(error) => error.code(),
    }
}

fn summary_kind(tool: ToolId) -> Option<crate::summaries::SummaryKind> {
    use crate::summaries::SummaryKind;

    match tool {
        ToolId::QueueSummary => Some(SummaryKind::Queue),
        ToolId::LibrarySummary => Some(SummaryKind::Library),
        ToolId::MonitoringStatus => Some(SummaryKind::Status),
        _ => None,
    }
}
