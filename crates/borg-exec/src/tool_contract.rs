use borg_apps::discovery::{GetAppArgs, GetAppResult, ListAppsArgs, ListAppsResult};
use borg_codemode::tools::{ExecuteCodeArgs, ExecuteCodeResult, SearchApisArgs};
use borg_core::ActorId;
use serde::{Deserialize, Serialize};

use crate::tool_runner::SendMessageArgs;
use borg_agent::admin_tools::{
    ActorAdminResult, CreateActorArgs, DisableActorArgs, ListActorsArgs, UpdateActorArgs,
    WhoAmIArgs,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "tool", content = "args", rename_all = "kebab-case")]
pub enum BorgToolCall {
    #[serde(rename = "Actors-sendMessage")]
    ActorsSendMessage(SendMessageArgs),

    #[serde(rename = "Actors-listActors")]
    ActorsListActors(ListActorsArgs),
    #[serde(rename = "Actors-whoAmI")]
    ActorsWhoAmI(WhoAmIArgs),
    #[serde(rename = "Actors-createActor")]
    ActorsCreateActor(CreateActorArgs),
    #[serde(rename = "Actors-updateActor")]
    ActorsUpdateActor(UpdateActorArgs),
    #[serde(rename = "Actors-disableActor")]
    ActorsDisableActor(DisableActorArgs),

    #[serde(rename = "CodeMode-searchApis")]
    CodeModeSearchApis(SearchApisArgs),
    #[serde(rename = "CodeMode-executeCode")]
    CodeModeExecuteCode(ExecuteCodeArgs),

    #[serde(rename = "Apps-listApps")]
    AppsListApps(ListAppsArgs),
    #[serde(rename = "Apps-getApp")]
    AppsGetApp(GetAppArgs),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BorgToolResult {
    ActorsAdmin(ActorAdminResult),
    CodeModeExecute(ExecuteCodeResult),
    CodeModeSearch(String),
    AppsList(ListAppsResult),
    AppsGet(GetAppResult),
    SendMessage(serde_json::Value),
}

impl borg_agent::ToolCall for BorgToolCall {}
impl borg_agent::ToolResult for BorgToolResult {}
