//! Account-capability dispatch; protocols retain their own native enforcement.
use super::{Caller, Commands, batch, profiles};
use crate::{
    arguments as args, error::ApiError, policy::Binding, registry::ToolId, source::ApiSource,
};
use serde_json::{Value, json};
impl Commands {
    pub(super) async fn broker_command(
        &self,
        caller: &Caller<'_>,
        tool: ToolId,
        input: Value,
    ) -> Result<Value, ApiError> {
        let source = self.sources.account_source().ok_or(ApiError::FORBIDDEN)?;
        let binding = caller
            .principal
            .binding
            .as_ref()
            .ok_or(ApiError::FORBIDDEN)?;

        if tool.is_write() {
            self.broker_write(caller, source, binding, tool, input)
                .await
        } else {
            self.broker_read(caller, source, binding, tool, input).await
        }
    }

    async fn broker_read(
        &self,
        caller: &Caller<'_>,
        source: &dyn ApiSource,
        binding: &Binding,
        tool: ToolId,
        input: Value,
    ) -> Result<Value, ApiError> {
        match tool {
            ToolId::SearchMedia => {
                source
                    .catalog()
                    .ok_or(ApiError::FORBIDDEN)?
                    .search(binding, args::decode(input)?)
                    .await
            }

            ToolId::MediaDetails => {
                let catalog = source.catalog().ok_or(ApiError::FORBIDDEN)?;
                let settings = catalog.public_settings(binding).await;

                catalog
                    .details(binding, args::decode(input)?, &settings)
                    .await
            }

            ToolId::RequestProfiles => {
                profiles::list(self, source, binding, args::decode(input)?).await
            }

            ToolId::MyQuota => {
                source
                    .accounts()
                    .ok_or(ApiError::FORBIDDEN)?
                    .quota(binding, args::decode::<args::Fresh>(input)?.no_cache)
                    .await
            }

            ToolId::MyRequests => {
                source
                    .requests()
                    .ok_or(ApiError::FORBIDDEN)?
                    .requests(binding, args::decode(input)?)
                    .await
            }

            ToolId::RequestStatus => {
                let args: args::Request = args::decode(input)?;
                let mut result = source
                    .requests()
                    .ok_or(ApiError::FORBIDDEN)?
                    .request(binding, args.request_id)
                    .await?;

                let object = result.as_object_mut().ok_or(ApiError::INVALID_RESPONSE)?;
                let provenance = object.entry("freshness").or_insert_with(|| json!({}));

                provenance
                    .as_object_mut()
                    .ok_or(ApiError::INVALID_RESPONSE)?
                    .insert("bypass_requested".to_owned(), json!(args.no_cache));
                Ok(result)
            }

            ToolId::ListIssues => {
                source
                    .issues()
                    .ok_or(ApiError::FORBIDDEN)?
                    .issues(binding, args::decode(input)?)
                    .await
            }

            ToolId::MediaDetailsBatch => {
                batch::details(self, caller, source, binding, args::decode(input)?).await
            }

            ToolId::CancelRequest
            | ToolId::CloseIssue
            | ToolId::CloseIssuesBatch
            | ToolId::ListMovies
            | ToolId::ListSeries
            | ToolId::ReportIssue
            | ToolId::RequestMediaBatch
            | ToolId::RequestMovie
            | ToolId::RequestSeries
            | ToolId::SearchLibrary
            | ToolId::QueueSummary
            | ToolId::LibrarySummary
            | ToolId::MonitoringStatus
            | ToolId::ServiceStatus
            | ToolId::UpcomingReleases => Err(ApiError::NOT_FOUND),
        }
    }

    async fn broker_write(
        &self,
        caller: &Caller<'_>,
        source: &dyn ApiSource,
        binding: &Binding,
        tool: ToolId,
        input: Value,
    ) -> Result<Value, ApiError> {
        match tool {
            ToolId::RequestMovie | ToolId::RequestSeries => {
                profiles::submit(self, source, binding, media_request(tool, input)?).await
            }

            ToolId::CancelRequest => {
                source
                    .requests()
                    .ok_or(ApiError::FORBIDDEN)?
                    .cancel(binding, args::decode::<args::Cancel>(input)?.request_id)
                    .await
            }

            ToolId::ReportIssue => {
                source
                    .issues()
                    .ok_or(ApiError::FORBIDDEN)?
                    .report(binding, args::decode(input)?)
                    .await
            }

            ToolId::CloseIssue => {
                source
                    .issues()
                    .ok_or(ApiError::FORBIDDEN)?
                    .close(binding, args::decode::<args::IssueId>(input)?.issue_id)
                    .await
            }

            ToolId::RequestMediaBatch => {
                batch::requests(self, caller, source, binding, args::decode(input)?).await
            }

            ToolId::CloseIssuesBatch => {
                batch::issues(self, caller, source, binding, args::decode(input)?).await
            }

            ToolId::ListIssues
            | ToolId::ListMovies
            | ToolId::ListSeries
            | ToolId::MediaDetails
            | ToolId::MediaDetailsBatch
            | ToolId::MyQuota
            | ToolId::MyRequests
            | ToolId::RequestProfiles
            | ToolId::RequestStatus
            | ToolId::SearchLibrary
            | ToolId::SearchMedia
            | ToolId::QueueSummary
            | ToolId::LibrarySummary
            | ToolId::MonitoringStatus
            | ToolId::ServiceStatus
            | ToolId::UpcomingReleases => Err(ApiError::NOT_FOUND),
        }
    }
}
fn media_request(tool: ToolId, input: Value) -> Result<args::MediaRequest, ApiError> {
    match tool {
        ToolId::RequestMovie => {
            let item: args::Movie = args::decode(input)?;

            Ok(args::MediaRequest {
                media_type: args::MediaType::Movie,
                tmdb_id: item.tmdb_id,
                seasons: Vec::new(),
                profile: item.profile,
            })
        }

        ToolId::RequestSeries => {
            let item: args::Series = args::decode(input)?;

            Ok(args::MediaRequest {
                media_type: args::MediaType::Tv,
                tmdb_id: item.tmdb_id,
                seasons: item.seasons,
                profile: item.profile,
            })
        }

        _ => Err(ApiError::NOT_FOUND),
    }
}
