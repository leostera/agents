use borg_apps::discovery::{GetAppArgs, GetAppResult, ListAppsArgs, ListAppsResult};
use borg_codemode::tools::{ExecuteCodeArgs, ExecuteCodeResult, SearchApisArgs};
use borg_core::ActorId;
use borg_fs::{FsToolArgs, FsToolOutput};
use borg_ports_tools::{CreatePortArgs, ListPortsArgs, PortAdminResult, UpdatePortArgs};
use borg_schedule::tools::{
    CreateJobArgs, JobIdArgs, ListJobsArgs, ListRunsArgs, ScheduleResult, UpdateJobArgs,
};
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

    #[serde(rename = "Schedule-listJobs")]
    ScheduleListJobs(ListJobsArgs),
    #[serde(rename = "Schedule-getJob")]
    ScheduleGetJob(JobIdArgs),
    #[serde(rename = "Schedule-createJob")]
    ScheduleCreateJob(CreateJobArgs),
    #[serde(rename = "Schedule-updateJob")]
    ScheduleUpdateJob(UpdateJobArgs),
    #[serde(rename = "Schedule-pauseJob")]
    SchedulePauseJob(JobIdArgs),
    #[serde(rename = "Schedule-resumeJob")]
    ScheduleResumeJob(JobIdArgs),
    #[serde(rename = "Schedule-cancelJob")]
    ScheduleCancelJob(JobIdArgs),
    #[serde(rename = "Schedule-listRuns")]
    ScheduleListRuns(ListRunsArgs),

    #[serde(rename = "Ports-listPorts")]
    PortsListPorts(ListPortsArgs),
    #[serde(rename = "Ports-createPort")]
    PortsCreatePort(CreatePortArgs),
    #[serde(rename = "Ports-updatePort")]
    PortsUpdatePort(UpdatePortArgs),

    #[serde(rename = "BorgFS-ls")]
    BorgFsLs(FsToolArgs),
    #[serde(rename = "BorgFS-search")]
    BorgFsSearch(FsToolArgs),
    #[serde(rename = "BorgFS-get")]
    BorgFsGet(FsToolArgs),
    #[serde(rename = "BorgFS-put")]
    BorgFsPut(FsToolArgs),
    #[serde(rename = "BorgFS-delete")]
    BorgFsDelete(FsToolArgs),
    #[serde(rename = "BorgFS-settings")]
    BorgFsSettings(FsToolArgs),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BorgToolResult {
    ActorsAdmin(ActorAdminResult),
    CodeModeExecute(ExecuteCodeResult),
    CodeModeSearch(String),
    AppsList(ListAppsResult),
    AppsGet(GetAppResult),
    Schedule(ScheduleResult),
    PortAdmin(PortAdminResult),
    BorgFs(FsToolOutput),
    SendMessage(serde_json::Value),
}

impl borg_agent::ToolCall for BorgToolCall {}
impl borg_agent::ToolResult for BorgToolResult {}
