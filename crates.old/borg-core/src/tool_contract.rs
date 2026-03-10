use crate::ids::ActorId;
use serde::{Deserialize, Serialize};

// --- Actors Messaging ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageArgs {
    pub target_actor_id: ActorId,
    pub text: String,
    #[serde(default)]
    pub reply_target_actor_id: Option<ActorId>,
    #[serde(default)]
    pub submission_id: Option<String>,
    #[serde(default)]
    pub in_reply_to_submission_id: Option<String>,
}

// --- Actor Admin ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListActorsArgs {
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhoAmIArgs {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateActorArgs {
    #[serde(default)]
    pub actor_id: Option<ActorId>,
    pub name: String,
    pub model: String,
    pub system_prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateActorArgs {
    pub actor_id: ActorId,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub system_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisableActorArgs {
    pub actor_id: ActorId,
}

// --- CodeMode ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchApisArgs {
    pub query: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteCodeArgs {
    pub hint: String,
    pub code: String,
}

// --- Apps ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListAppsArgs {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetAppArgs {
    pub id: String,
}

// --- Exhaustive Contract ---

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

    /// Fallback for dynamic/external tools not yet in the exhaustive enum
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BorgToolResult {
    Ok(serde_json::Value),
    Error(String),
}
