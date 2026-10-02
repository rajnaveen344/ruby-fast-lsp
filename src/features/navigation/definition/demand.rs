//! Definition requests that arrive while their target is still indexing:
//! derive the exact project and dependency demand keys at the cursor, ask the
//! indexer to prioritize them, and retry the lookup as each demand completes.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use log::info;
use ruby_analysis::core::MethodReceiver;
use ruby_analysis::indexer::{Identifier, RubyPrismAnalyzer};
use tower_lsp::jsonrpc::{Error as LspError, ErrorCode, Result as LspResult};
use tower_lsp::lsp_types::{GotoDefinitionResponse, Position, Url};

use super::find_definition_at_position;
use crate::loader::scheduling::navigation_demand::{
    NavigationDemandOutcome, NavigationDemandStage,
};
use crate::server::{RubyLanguageServer, Workspace};
use crate::utils::lsp::source_position;

const PROJECT_NAVIGATION_DEMAND_WAIT: Duration = Duration::from_secs(5);
const DEPENDENCY_NAVIGATION_DEMAND_WAIT: Duration = Duration::from_secs(15);

pub(crate) fn navigation_demand_keys_at_position(
    server: &RubyLanguageServer,
    uri: &Url,
    position: Position,
) -> Option<DefinitionNavigationDemandKeys> {
    let content = {
        let documents = server.documents.read();
        let content = documents.get(uri)?.read().content.clone();
        content
    };
    definition_navigation_demand_keys(uri, position, &content)
}

/// Wait for the demanded project or dependency input and retry the definition
/// after each completion. Returns an indexing-in-progress error when the
/// definition still depends on pending indexing.
pub(super) async fn wait_for_demanded_definition(
    server: &RubyLanguageServer,
    project: &Workspace,
    demand_keys: &DefinitionNavigationDemandKeys,
    uri: &Url,
    position: Position,
) -> LspResult<Option<GotoDefinitionResponse>> {
    let mut definition = None;
    let snapshot = project.indexing_status.snapshot();
    type DemandWait = Pin<
        Box<
            dyn Future<
                    Output = (
                        NavigationDemandStage,
                        Result<NavigationDemandOutcome, tokio::time::error::Elapsed>,
                    ),
                > + Send,
        >,
    >;
    let mut project_wait: Option<DemandWait> =
        if snapshot.generation > 0 && snapshot.phase.project_navigation_pending() {
            demand_keys.project_key.as_deref().map(|key| {
                let ticket = project.navigation_demands.request(
                    snapshot.generation,
                    NavigationDemandStage::Project,
                    key,
                );
                Box::pin(async move {
                    (
                        NavigationDemandStage::Project,
                        tokio::time::timeout(PROJECT_NAVIGATION_DEMAND_WAIT, ticket.wait()).await,
                    )
                }) as DemandWait
            })
        } else {
            None
        };
    let mut dependency_wait: Option<DemandWait> = if snapshot.generation > 0
        && snapshot.phase.dependency_navigation_pending()
    {
        demand_keys.dependency_key.as_deref().map(|key| {
            let ticket = project.navigation_demands.request(
                snapshot.generation,
                NavigationDemandStage::Dependency,
                key,
            );
            Box::pin(async move {
                (
                    NavigationDemandStage::Dependency,
                    tokio::time::timeout(DEPENDENCY_NAVIGATION_DEMAND_WAIT, ticket.wait()).await,
                )
            }) as DemandWait
        })
    } else {
        None
    };
    let mut deferred_reason = None;
    if project_wait.is_some() || dependency_wait.is_some() {
        info!(
            "Goto definition waiting for navigation demand (project_timeout={:?}, dependency_timeout={:?}) at {:?}",
            PROJECT_NAVIGATION_DEMAND_WAIT,
            DEPENDENCY_NAVIGATION_DEMAND_WAIT,
            position
        );
    }
    while definition.is_none() && (project_wait.is_some() || dependency_wait.is_some()) {
        let (stage, outcome) = match (&mut project_wait, &mut dependency_wait) {
            (Some(project_future), Some(dependency_future)) => {
                tokio::select! {
                    outcome = project_future.as_mut() => {
                        project_wait = None;
                        outcome
                    }
                    outcome = dependency_future.as_mut() => {
                        dependency_wait = None;
                        outcome
                    }
                }
            }
            (Some(project_future), None) => {
                let outcome = project_future.as_mut().await;
                project_wait = None;
                outcome
            }
            (None, Some(dependency_future)) => {
                let outcome = dependency_future.as_mut().await;
                dependency_wait = None;
                outcome
            }
            (None, None) => unreachable_invariant!(
                what = "navigation wait loop entered without a future",
                why = "the loop predicate and exact branch observe the same local options",
                fix = "keep demand-future removal inside this match",
            ),
        };
        match outcome {
            Ok(
                NavigationDemandOutcome::TargetProcessed | NavigationDemandOutcome::StageComplete,
            ) => {
                definition = find_definition_at_position(server, uri.clone(), position).await;
            }
            Ok(NavigationDemandOutcome::Superseded) => {
                return Err(LspError::content_modified());
            }
            Ok(NavigationDemandOutcome::Cancelled) => {
                return Err(LspError::request_cancelled());
            }
            Ok(NavigationDemandOutcome::Saturated) => {
                deferred_reason = Some(match stage {
                    NavigationDemandStage::Project => "the bounded project-demand queue is full",
                    NavigationDemandStage::Dependency => {
                        "the bounded dependency-demand queue is full"
                    }
                });
            }
            Err(_) => {
                deferred_reason = Some(match stage {
                    NavigationDemandStage::Project => {
                        "the requested project input is still indexing"
                    }
                    NavigationDemandStage::Dependency => {
                        "the requested dependency input is still indexing"
                    }
                });
            }
        }
    }
    if definition.is_none() {
        let phase = project.indexing_status.snapshot().phase;
        if phase.project_navigation_pending() || phase.dependency_navigation_pending() {
            let reason = deferred_reason
                .unwrap_or("the requested definition still depends on broader indexing");
            info!(
                "Goto definition deferred while indexing ({reason}) at {:?}",
                position
            );
            return Err(indexing_in_progress_error(project, reason));
        }
    }
    Ok(definition)
}

fn indexing_in_progress_error(project: &Workspace, reason: &str) -> LspError {
    LspError {
        code: ErrorCode::ServerError(-32802),
        message: format!(
            "Ruby Fast LSP is still indexing {}: {reason}",
            project.root_path.display()
        )
        .into(),
        data: Some(serde_json::json!({ "retriggerRequest": true })),
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct DefinitionNavigationDemandKeys {
    pub(crate) project_key: Option<String>,
    pub(crate) dependency_key: Option<String>,
}

pub(crate) fn definition_navigation_demand_keys(
    uri: &Url,
    position: Position,
    content: &str,
) -> Option<DefinitionNavigationDemandKeys> {
    let analyzer = RubyPrismAnalyzer::new(uri.clone(), content.to_string());
    let (identifier, _, ancestors, _, _) =
        analyzer.get_identifier_at_position(source_position(position));
    let (project_constant, dependency_constant) = match identifier? {
        Identifier::RubyConstant { iden, .. } => (iden.last().cloned(), iden.first().cloned()),
        Identifier::RubyMethod {
            namespace,
            receiver,
            ..
        } => match receiver {
            MethodReceiver::Constant(parts) => (parts.last().cloned(), parts.first().cloned()),
            MethodReceiver::None | MethodReceiver::SelfReceiver | MethodReceiver::Super => {
                (namespace.last().cloned(), namespace.first().cloned())
            }
            MethodReceiver::LocalVariable(_)
            | MethodReceiver::InstanceVariable(_)
            | MethodReceiver::ClassVariable(_)
            | MethodReceiver::GlobalVariable(_)
            | MethodReceiver::MethodCall { .. }
            | MethodReceiver::Literal(_)
            | MethodReceiver::Expression => (ancestors.last().cloned(), ancestors.first().cloned()),
        },
        Identifier::YardType { type_name, .. } => {
            let mut parts = type_name.split("::").filter(|part| !part.is_empty());
            let first = parts.next().map(ToString::to_string);
            let last = parts.last().or(first.as_deref()).map(ToString::to_string);
            return normalized_definition_navigation_keys(last.as_deref(), first.as_deref());
        }
        Identifier::RubyLocalVariable { .. }
        | Identifier::RubyInstanceVariable { .. }
        | Identifier::RubyClassVariable { .. }
        | Identifier::RubyGlobalVariable { .. } => return None,
    };
    normalized_definition_navigation_keys(
        project_constant
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
        dependency_constant
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
    )
}

fn normalized_definition_navigation_keys(
    project_name: Option<&str>,
    dependency_name: Option<&str>,
) -> Option<DefinitionNavigationDemandKeys> {
    let project_key = project_name
        .map(crate::loader::scheduling::navigation_demand::normalize_navigation_key)
        .filter(|key| !key.is_empty());
    let dependency_key = dependency_name
        .map(crate::loader::scheduling::navigation_demand::normalize_navigation_key)
        .filter(|key| !key.is_empty());
    (project_key.is_some() || dependency_key.is_some()).then_some(DefinitionNavigationDemandKeys {
        project_key,
        dependency_key,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_definition_request_exposes_exact_project_and_dependency_keys() {
        let source = "ExampleApp::Platform::Users::AccountRecord.find_by_key(key)\n";
        let uri = crate::test::harness::fixture_uri("/project/caller.rb");

        let demand = definition_navigation_demand_keys(
            &uri,
            Position::new(
                0,
                u32::try_from(source.find("AccountRecord").unwrap() + 2).unwrap(),
            ),
            source,
        )
        .unwrap();

        assert_eq!(demand.project_key.as_deref(), Some("accountrecord"));
        assert_eq!(demand.dependency_key.as_deref(), Some("exampleapp"));
    }

    #[test]
    fn constant_receiver_method_request_prioritizes_its_owning_constant() {
        let source = "BSON::ObjectId.new\n";
        let uri = crate::test::harness::fixture_uri("/project/caller.rb");

        let demand = definition_navigation_demand_keys(&uri, Position::new(0, 16), source).unwrap();

        assert_eq!(demand.project_key.as_deref(), Some("objectid"));
        assert_eq!(demand.dependency_key.as_deref(), Some("bson"));
    }
}
