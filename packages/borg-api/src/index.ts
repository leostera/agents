export type ProviderRecord = {
  provider: string;
  api_key: string;
  enabled: boolean;
  tokens_used: number;
  last_used?: string | null;
  default_text_model?: string | null;
  default_audio_model?: string | null;
  created_at: string;
  updated_at: string;
};

export type SessionRecord = {
  session_id: string;
  users: string[];
  port: string;
  updated_at: string;
};

export type ProvidersResponse = {
  providers?: ProviderRecord[];
};

export type AppRecord = {
  app_id: string;
  name: string;
  slug: string;
  description: string;
  status: string;
  built_in: boolean;
  created_at: string;
  updated_at: string;
};

export type AppsResponse = {
  apps?: AppRecord[];
};

export type AppResponse = {
  app?: AppRecord;
};

export type AppCapabilityRecord = {
  capability_id: string;
  app_id: string;
  name: string;
  hint: string;
  mode: string;
  instructions: string;
  status: string;
  created_at: string;
  updated_at: string;
};

export type AppCapabilitiesResponse = {
  capabilities?: AppCapabilityRecord[];
};

export type AppCapabilityResponse = {
  capability?: AppCapabilityRecord;
};

export type HealthResponse = {
  status?: string;
};

export type SessionsResponse = {
  sessions?: SessionRecord[];
};

export type SessionResponse = {
  session?: SessionRecord;
};

export type AgentSpecRecord = {
  agent_id: string;
  name: string;
  enabled: boolean;
  default_provider_id?: string | null;
  model: string;
  system_prompt: string;
  updated_at: string;
};

export type AgentSpecsResponse = {
  agent_specs?: AgentSpecRecord[];
};

export type AgentSpecResponse = {
  agent_spec?: AgentSpecRecord;
};

export type UserRecord = {
  user_key: string;
  profile: Record<string, unknown>;
  created_at: string;
  updated_at: string;
};

export type UsersResponse = {
  users?: UserRecord[];
};

export type UserResponse = {
  user?: UserRecord;
};

export type SessionMessagesResponse = {
  messages?: Record<string, unknown>[];
};

export type PortSetting = {
  key: string;
  value: string;
};

export type PortRecord = {
  port_id: string;
  provider: string;
  port_name: string;
  enabled: boolean;
  allows_guests: boolean;
  default_agent_id?: string | null;
  settings: Record<string, unknown>;
  active_sessions: number;
  updated_at?: string | null;
};

export type PortsResponse = {
  ports?: PortRecord[];
};

export type PortSettingsResponse = {
  settings?: PortSetting[];
};

export type PortSettingResponse = {
  value?: string;
};

export type PortBinding = {
  conversation_key: string;
  session_id: string;
  agent_id?: string | null;
};

export type PortBindingsResponse = {
  bindings?: PortBinding[];
};

export type ProviderModelsResponse = {
  provider?: string;
  models?: string[];
  default_text_model?: string | null;
  default_audio_model?: string | null;
};

export type MemoryEntity = {
  entity_id: string;
  entity_type: string;
  label: string;
  props?: Record<string, unknown>;
};

export type MemorySearchResponse = {
  entities?: MemoryEntity[];
};

export type MemoryEntityResponse = {
  entity?: MemoryEntity;
};

export type MemoryExplorerEdge = {
  source: string;
  target: string;
  relation: string;
};

export type MemoryExplorerResponse = {
  entities?: MemoryEntity[];
  edges?: MemoryExplorerEdge[];
};

export type TaskGraphReview = {
  submitted_at?: string | null;
  approved_at?: string | null;
  changes_requested_at?: string | null;
};

export type TaskGraphTask = {
  uri: string;
  title: string;
  description: string;
  definition_of_done: string;
  status: "pending" | "doing" | "review" | "done" | "discarded";
  assignee_agent_id: string;
  assignee_session_uri: string;
  reviewer_agent_id: string;
  reviewer_session_uri: string;
  labels: string[];
  parent_uri?: string | null;
  blocked_by: string[];
  duplicate_of?: string | null;
  references: string[];
  review: TaskGraphReview;
  created_at: string;
  updated_at: string;
};

export type TaskGraphComment = {
  id: string;
  task_uri: string;
  author_session_uri: string;
  body: string;
  created_at: string;
};

export type TaskGraphEvent = {
  id: string;
  task_uri: string;
  actor_session_uri: string;
  type: string;
  data: Record<string, unknown>;
  created_at: string;
};

export type TaskGraphTasksResponse = {
  tasks?: TaskGraphTask[];
  next_cursor?: string | null;
};

export type TaskGraphTaskResponse = {
  task?: TaskGraphTask;
};

export type TaskGraphCommentsResponse = {
  comments?: TaskGraphComment[];
  next_cursor?: string | null;
};

export type TaskGraphEventsResponse = {
  events?: TaskGraphEvent[];
  next_cursor?: string | null;
};

export type TaskGraphChildrenResponse = {
  children?: TaskGraphTask[];
  next_cursor?: string | null;
};

export type LlmCallRecord = {
  call_id: string;
  provider: string;
  capability: string;
  model: string;
  success: boolean;
  status_code?: number | null;
  status_reason?: string | null;
  http_reason?: string | null;
  error?: string | null;
  latency_ms?: number | null;
  sent_at: string;
  received_at?: string | null;
  request_json: unknown;
  response_json: unknown;
  response_body: string;
};

export type LlmCallsResponse = {
  llm_calls?: LlmCallRecord[];
};

export type LlmCallResponse = {
  llm_call?: LlmCallRecord;
};

export type ToolCallRecord = {
  call_id: string;
  session_id: string;
  tool_name: string;
  arguments_json: unknown;
  output_json: unknown;
  success: boolean;
  error?: string | null;
  duration_ms?: number | null;
  called_at: string;
};

export type ToolCallsResponse = {
  tool_calls?: ToolCallRecord[];
};

export type ToolCallResponse = {
  tool_call?: ToolCallRecord;
};

export class BorgApiError extends Error {
  status?: number;
  bodyText?: string;

  constructor(
    message: string,
    options: { status?: number; bodyText?: string } = {}
  ) {
    super(message);
    this.name = "BorgApiError";
    this.status = options.status;
    this.bodyText = options.bodyText;
  }
}

export type BorgApiClientOptions = {
  baseUrl?: string;
};

function trimTrailingSlash(url: string): string {
  return url.replace(/\/+$/, "");
}

function resolveDefaultBaseUrl(): string {
  if (typeof window === "undefined") return "";

  const fromEnv =
    (import.meta as unknown as { env?: Record<string, string | undefined> }).env
      ?.VITE_BORG_API_BASE_URL ?? "";
  if (fromEnv.trim()) return trimTrailingSlash(fromEnv.trim());

  const { origin, hostname, protocol, port } = window.location;
  const isLocal = hostname === "localhost" || hostname === "127.0.0.1";
  if (isLocal && (port === "5173" || port === "4173")) {
    return `${protocol}//${hostname}:8080`;
  }
  return origin;
}

async function readResponseBody(response: Response): Promise<string> {
  try {
    return await response.text();
  } catch {
    return "";
  }
}

export class BorgApiClient {
  private readonly baseUrl: string;

  constructor(options: BorgApiClientOptions = {}) {
    this.baseUrl = trimTrailingSlash(
      options.baseUrl ?? resolveDefaultBaseUrl()
    );
  }

  private url(path: string): string {
    return `${this.baseUrl}${path}`;
  }

  private async request(path: string, init?: RequestInit): Promise<Response> {
    let response: Response;
    try {
      response = await fetch(this.url(path), init);
    } catch (error) {
      const fallbackOrigin =
        typeof window !== "undefined" ? window.location.origin : "";
      const message =
        error instanceof TypeError
          ? `Borg API unavailable at ${this.baseUrl || fallbackOrigin}`
          : "Unable to reach Borg API";
      throw new BorgApiError(message);
    }
    if (!response.ok) {
      const bodyText = await readResponseBody(response);
      throw new BorgApiError(`Request failed (${response.status})`, {
        status: response.status,
        bodyText,
      });
    }
    return response;
  }

  private async requestJson<T>(path: string, init?: RequestInit): Promise<T> {
    const response = await this.request(path, init);
    const contentType = response.headers.get("content-type") ?? "";
    if (!contentType.includes("application/json")) {
      const bodyText = await readResponseBody(response);
      throw new BorgApiError("Endpoint returned non-JSON response", {
        status: response.status,
        bodyText,
      });
    }
    return (await response.json()) as T;
  }

  async listProviders(limit = 100): Promise<ProviderRecord[]> {
    const data = await this.requestJson<ProvidersResponse>(
      `/api/providers?limit=${limit}`
    );
    return Array.isArray(data.providers) ? data.providers : [];
  }

  async listApps(limit = 100): Promise<AppRecord[]> {
    const data = await this.requestJson<AppsResponse>(
      `/api/apps?limit=${limit}`
    );
    return Array.isArray(data.apps) ? data.apps : [];
  }

  async getApp(appId: string): Promise<AppRecord | null> {
    try {
      const data = await this.requestJson<AppResponse>(
        `/api/apps/${encodeURIComponent(appId)}`
      );
      return data.app ?? null;
    } catch (error) {
      if (error instanceof BorgApiError && error.status === 404) {
        return null;
      }
      throw error;
    }
  }

  async upsertApp(
    appId: string,
    payload: {
      name: string;
      slug: string;
      description?: string;
      status?: string;
    }
  ): Promise<void> {
    await this.request(`/api/apps/${encodeURIComponent(appId)}`, {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(payload),
    });
  }

  async listAppCapabilities(
    appId: string,
    limit = 100
  ): Promise<AppCapabilityRecord[]> {
    const data = await this.requestJson<AppCapabilitiesResponse>(
      `/api/apps/${encodeURIComponent(appId)}/capabilities?limit=${limit}`
    );
    if (!Array.isArray(data.capabilities)) {
      throw new BorgApiError(
        "Invalid app capabilities response payload: expected `capabilities` array"
      );
    }
    return data.capabilities;
  }

  async getAppCapability(
    appId: string,
    capabilityId: string
  ): Promise<AppCapabilityRecord | null> {
    try {
      const data = await this.requestJson<AppCapabilityResponse>(
        `/api/apps/${encodeURIComponent(appId)}/capabilities/${encodeURIComponent(capabilityId)}`
      );
      return data.capability ?? null;
    } catch (error) {
      if (error instanceof BorgApiError && error.status === 404) {
        return null;
      }
      throw error;
    }
  }

  async upsertAppCapability(
    appId: string,
    capabilityId: string,
    payload: {
      name: string;
      hint?: string;
      mode?: string;
      instructions?: string;
      status?: string;
    }
  ): Promise<void> {
    await this.request(
      `/api/apps/${encodeURIComponent(appId)}/capabilities/${encodeURIComponent(capabilityId)}`,
      {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(payload),
      }
    );
  }

  async deleteAppCapability(
    appId: string,
    capabilityId: string,
    options: { ignoreNotFound?: boolean } = {}
  ): Promise<void> {
    try {
      await this.request(
        `/api/apps/${encodeURIComponent(appId)}/capabilities/${encodeURIComponent(capabilityId)}`,
        {
          method: "DELETE",
        }
      );
    } catch (error) {
      if (
        options.ignoreNotFound &&
        error instanceof BorgApiError &&
        error.status === 404
      ) {
        return;
      }
      throw error;
    }
  }

  async deleteApp(
    appId: string,
    options: { ignoreNotFound?: boolean } = {}
  ): Promise<void> {
    try {
      await this.request(`/api/apps/${encodeURIComponent(appId)}`, {
        method: "DELETE",
      });
    } catch (error) {
      if (
        options.ignoreNotFound &&
        error instanceof BorgApiError &&
        error.status === 404
      ) {
        return;
      }
      throw error;
    }
  }

  async health(): Promise<boolean> {
    const data = await this.requestJson<HealthResponse>("/health");
    return data.status === "ok";
  }

  async listTaskGraphTasks(
    params: { limit?: number; cursor?: string | null } = {}
  ): Promise<{ tasks: TaskGraphTask[]; nextCursor: string | null }> {
    const searchParams = new URLSearchParams({
      limit: String(params.limit ?? 500),
    });
    if (params.cursor) {
      searchParams.set("cursor", params.cursor);
    }
    const data = await this.requestJson<TaskGraphTasksResponse>(
      `/api/taskgraph/tasks?${searchParams.toString()}`
    );
    return {
      tasks: Array.isArray(data.tasks) ? data.tasks : [],
      nextCursor:
        typeof data.next_cursor === "string" && data.next_cursor.length > 0
          ? data.next_cursor
          : null,
    };
  }

  async getTaskGraphTask(taskUri: string): Promise<TaskGraphTask | null> {
    try {
      const data = await this.requestJson<TaskGraphTaskResponse>(
        `/api/taskgraph/tasks/${encodeURIComponent(taskUri)}`
      );
      return data.task ?? null;
    } catch (error) {
      if (error instanceof BorgApiError && error.status === 404) {
        return null;
      }
      throw error;
    }
  }

  async listTaskGraphComments(
    taskUri: string,
    params: { limit?: number; cursor?: string | null } = {}
  ): Promise<{ comments: TaskGraphComment[]; nextCursor: string | null }> {
    const searchParams = new URLSearchParams({
      limit: String(params.limit ?? 200),
    });
    if (params.cursor) {
      searchParams.set("cursor", params.cursor);
    }
    const data = await this.requestJson<TaskGraphCommentsResponse>(
      `/api/taskgraph/tasks/${encodeURIComponent(taskUri)}/comments?${searchParams.toString()}`
    );
    return {
      comments: Array.isArray(data.comments) ? data.comments : [],
      nextCursor:
        typeof data.next_cursor === "string" && data.next_cursor.length > 0
          ? data.next_cursor
          : null,
    };
  }

  async listTaskGraphEvents(
    taskUri: string,
    params: { limit?: number; cursor?: string | null } = {}
  ): Promise<{ events: TaskGraphEvent[]; nextCursor: string | null }> {
    const searchParams = new URLSearchParams({
      limit: String(params.limit ?? 200),
    });
    if (params.cursor) {
      searchParams.set("cursor", params.cursor);
    }
    const data = await this.requestJson<TaskGraphEventsResponse>(
      `/api/taskgraph/tasks/${encodeURIComponent(taskUri)}/events?${searchParams.toString()}`
    );
    return {
      events: Array.isArray(data.events) ? data.events : [],
      nextCursor:
        typeof data.next_cursor === "string" && data.next_cursor.length > 0
          ? data.next_cursor
          : null,
    };
  }

  async listTaskGraphChildren(
    taskUri: string,
    params: { limit?: number; cursor?: string | null } = {}
  ): Promise<{ children: TaskGraphTask[]; nextCursor: string | null }> {
    const searchParams = new URLSearchParams({
      limit: String(params.limit ?? 200),
    });
    if (params.cursor) {
      searchParams.set("cursor", params.cursor);
    }
    const data = await this.requestJson<TaskGraphChildrenResponse>(
      `/api/taskgraph/tasks/${encodeURIComponent(taskUri)}/children?${searchParams.toString()}`
    );
    return {
      children: Array.isArray(data.children) ? data.children : [],
      nextCursor:
        typeof data.next_cursor === "string" && data.next_cursor.length > 0
          ? data.next_cursor
          : null,
    };
  }

  async listSessions(limit = 100): Promise<SessionRecord[]> {
    const data = await this.requestJson<SessionsResponse>(
      `/api/sessions?limit=${limit}`
    );
    return Array.isArray(data.sessions) ? data.sessions : [];
  }

  async getSession(sessionId: string): Promise<SessionRecord | null> {
    try {
      const data = await this.requestJson<SessionResponse>(
        `/api/sessions/${encodeURIComponent(sessionId)}`
      );
      return data.session ?? null;
    } catch (error) {
      if (error instanceof BorgApiError && error.status === 404) {
        return null;
      }
      throw error;
    }
  }

  async listSessionMessages(
    sessionId: string,
    params: { from?: number; limit?: number } = {}
  ): Promise<Record<string, unknown>[]> {
    const searchParams = new URLSearchParams({
      from: String(params.from ?? 0),
      limit: String(params.limit ?? 200),
    });
    const data = await this.requestJson<SessionMessagesResponse>(
      `/api/sessions/${encodeURIComponent(sessionId)}/messages?${searchParams.toString()}`
    );
    return Array.isArray(data.messages) ? data.messages : [];
  }

  async listAgentSpecs(limit = 100): Promise<AgentSpecRecord[]> {
    const data = await this.requestJson<AgentSpecsResponse>(
      `/api/agents/specs?limit=${limit}`
    );
    return Array.isArray(data.agent_specs) ? data.agent_specs : [];
  }

  async getAgentSpec(agentId: string): Promise<AgentSpecRecord | null> {
    try {
      const data = await this.requestJson<AgentSpecResponse>(
        `/api/agents/specs/${encodeURIComponent(agentId)}`
      );
      return data.agent_spec ?? null;
    } catch (error) {
      if (error instanceof BorgApiError && error.status === 404) {
        return null;
      }
      throw error;
    }
  }

  async upsertAgentSpec(payload: {
    agentId: string;
    name: string;
    defaultProviderId?: string | null;
    model: string;
    systemPrompt: string;
  }): Promise<void> {
    await this.request(
      `/api/agents/specs/${encodeURIComponent(payload.agentId)}`,
      {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          name: payload.name,
          default_provider_id: payload.defaultProviderId ?? null,
          model: payload.model,
          system_prompt: payload.systemPrompt,
        }),
      }
    );
  }

  async deleteAgentSpec(
    agentId: string,
    options: { ignoreNotFound?: boolean } = {}
  ): Promise<void> {
    try {
      await this.request(`/api/agents/specs/${encodeURIComponent(agentId)}`, {
        method: "DELETE",
      });
    } catch (error) {
      if (
        options.ignoreNotFound &&
        error instanceof BorgApiError &&
        error.status === 404
      ) {
        return;
      }
      throw error;
    }
  }

  async setAgentSpecEnabled(agentId: string, enabled: boolean): Promise<void> {
    await this.request(
      `/api/agents/specs/${encodeURIComponent(agentId)}/enabled`,
      {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ enabled }),
      }
    );
  }

  async listUsers(limit = 100): Promise<UserRecord[]> {
    const data = await this.requestJson<UsersResponse>(
      `/api/users?limit=${limit}`
    );
    return Array.isArray(data.users) ? data.users : [];
  }

  async getUser(userKey: string): Promise<UserRecord | null> {
    try {
      const data = await this.requestJson<UserResponse>(
        `/api/users/${encodeURIComponent(userKey)}`
      );
      return data.user ?? null;
    } catch (error) {
      if (error instanceof BorgApiError && error.status === 404) {
        return null;
      }
      throw error;
    }
  }

  async upsertUser(
    userKey: string,
    profile: Record<string, unknown>
  ): Promise<void> {
    await this.request(`/api/users`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ user_key: userKey, profile }),
    });
  }

  async patchUser(
    userKey: string,
    profile: Record<string, unknown>
  ): Promise<void> {
    await this.request(`/api/users/${encodeURIComponent(userKey)}`, {
      method: "PATCH",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ profile }),
    });
  }

  async deleteUser(
    userKey: string,
    options: { ignoreNotFound?: boolean } = {}
  ): Promise<void> {
    try {
      await this.request(`/api/users/${encodeURIComponent(userKey)}`, {
        method: "DELETE",
      });
    } catch (error) {
      if (
        options.ignoreNotFound &&
        error instanceof BorgApiError &&
        error.status === 404
      ) {
        return;
      }
      throw error;
    }
  }

  async listPortSettings(portUri: string, limit = 200): Promise<PortSetting[]> {
    const data = await this.requestJson<PortSettingsResponse>(
      `/api/ports/${encodeURIComponent(portUri)}/settings?limit=${limit}`
    );
    return Array.isArray(data.settings) ? data.settings : [];
  }

  async listPorts(limit = 200): Promise<PortRecord[]> {
    const data = await this.requestJson<PortsResponse>(
      `/api/ports?limit=${limit}`
    );
    return Array.isArray(data.ports) ? data.ports : [];
  }

  async upsertPort(
    portUri: string,
    payload: {
      provider: string;
      enabled: boolean;
      allows_guests: boolean;
      default_agent_id?: string | null;
      settings?: Record<string, unknown>;
    }
  ): Promise<void> {
    await this.request(`/api/ports/${encodeURIComponent(portUri)}`, {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(payload),
    });
  }

  async deletePort(
    portUri: string,
    options: { ignoreNotFound?: boolean } = {}
  ): Promise<void> {
    try {
      await this.request(`/api/ports/${encodeURIComponent(portUri)}`, {
        method: "DELETE",
      });
    } catch (error) {
      if (
        options.ignoreNotFound &&
        error instanceof BorgApiError &&
        error.status === 404
      ) {
        return;
      }
      throw error;
    }
  }

  async getPortSetting(portUri: string, key: string): Promise<string | null> {
    try {
      const data = await this.requestJson<PortSettingResponse>(
        `/api/ports/${encodeURIComponent(portUri)}/settings/${encodeURIComponent(key)}`
      );
      return typeof data.value === "string" ? data.value : null;
    } catch (error) {
      if (error instanceof BorgApiError && error.status === 404) {
        return null;
      }
      throw error;
    }
  }

  async upsertPortSetting(
    portUri: string,
    key: string,
    value: string
  ): Promise<void> {
    await this.request(
      `/api/ports/${encodeURIComponent(portUri)}/settings/${encodeURIComponent(key)}`,
      {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ value }),
      }
    );
  }

  async deletePortSetting(
    portUri: string,
    key: string,
    options: { ignoreNotFound?: boolean } = {}
  ): Promise<void> {
    try {
      await this.request(
        `/api/ports/${encodeURIComponent(portUri)}/settings/${encodeURIComponent(key)}`,
        { method: "DELETE" }
      );
    } catch (error) {
      if (
        options.ignoreNotFound &&
        error instanceof BorgApiError &&
        error.status === 404
      ) {
        return;
      }
      throw error;
    }
  }

  async listPortBindings(portUri: string, limit = 200): Promise<PortBinding[]> {
    const data = await this.requestJson<PortBindingsResponse>(
      `/api/ports/${encodeURIComponent(portUri)}/bindings?limit=${limit}`
    );
    return Array.isArray(data.bindings) ? data.bindings : [];
  }

  async upsertProviderApiKey(provider: string, apiKey: string): Promise<void> {
    await this.request(`/api/providers/${provider}`, {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ api_key: apiKey }),
    });
  }

  async upsertProvider(payload: {
    provider: string;
    apiKey: string;
    enabled?: boolean;
    defaultTextModel?: string | null;
    defaultAudioModel?: string | null;
  }): Promise<void> {
    await this.request(
      `/api/providers/${encodeURIComponent(payload.provider)}`,
      {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          api_key: payload.apiKey,
          enabled: payload.enabled,
          default_text_model: payload.defaultTextModel,
          default_audio_model: payload.defaultAudioModel,
        }),
      }
    );
  }

  async listProviderModels(provider: string): Promise<string[]> {
    const data = await this.requestJson<ProviderModelsResponse>(
      `/api/providers/${encodeURIComponent(provider)}/models`
    );
    return Array.isArray(data.models) ? data.models : [];
  }

  async getProviderModels(provider: string): Promise<ProviderModelsResponse> {
    return this.requestJson<ProviderModelsResponse>(
      `/api/providers/${encodeURIComponent(provider)}/models`
    );
  }

  async deleteProvider(
    provider: string,
    options: { ignoreNotFound?: boolean } = {}
  ): Promise<void> {
    try {
      await this.request(`/api/providers/${provider}`, { method: "DELETE" });
    } catch (error) {
      if (
        options.ignoreNotFound &&
        error instanceof BorgApiError &&
        error.status === 404
      ) {
        return;
      }
      throw error;
    }
  }

  async startOpenAiDeviceCode(): Promise<void> {
    await this.request("/api/providers/openai/device-code/start", {
      method: "POST",
    });
  }

  async searchMemory(params: {
    q: string;
    type?: string;
    limit?: number;
  }): Promise<MemoryEntity[]> {
    const searchParams = new URLSearchParams({
      q: params.q,
      limit: String(params.limit ?? 50),
    });
    if (params.type && params.type.trim()) {
      searchParams.set("type", params.type.trim());
    }
    const data = await this.requestJson<MemorySearchResponse>(
      `/memory/search?${searchParams.toString()}`
    );
    return Array.isArray(data.entities) ? data.entities : [];
  }

  async getMemoryEntity(uri: string): Promise<MemoryEntity | null> {
    try {
      const data = await this.requestJson<MemoryEntityResponse>(
        `/memory/entities/${encodeURIComponent(uri)}`
      );
      return data.entity ?? null;
    } catch (error) {
      if (error instanceof BorgApiError && error.status === 404) {
        return null;
      }
      throw error;
    }
  }

  async exploreMemory(params: {
    query: string;
    limit?: number;
    maxNodes?: number;
  }): Promise<{ entities: MemoryEntity[]; edges: MemoryExplorerEdge[] }> {
    const searchParams = new URLSearchParams({
      query: params.query.trim(),
      limit: String(params.limit ?? 25),
      max_nodes: String(params.maxNodes ?? 300),
    });
    const data = await this.requestJson<MemoryExplorerResponse>(
      `/api/memory/explorer?${searchParams.toString()}`
    );
    return {
      entities: Array.isArray(data.entities) ? data.entities : [],
      edges: Array.isArray(data.edges) ? data.edges : [],
    };
  }

  async listLlmCalls(limit = 500): Promise<LlmCallRecord[]> {
    const data = await this.requestJson<LlmCallsResponse>(
      `/api/observability/llm-calls?limit=${limit}`
    );
    return Array.isArray(data.llm_calls) ? data.llm_calls : [];
  }

  async getLlmCall(callId: string): Promise<LlmCallRecord | null> {
    try {
      const data = await this.requestJson<LlmCallResponse>(
        `/api/observability/llm-calls/${encodeURIComponent(callId)}`
      );
      return data.llm_call ?? null;
    } catch (error) {
      if (error instanceof BorgApiError && error.status === 404) {
        return null;
      }
      throw error;
    }
  }

  async listToolCalls(limit = 500): Promise<ToolCallRecord[]> {
    const data = await this.requestJson<ToolCallsResponse>(
      `/api/observability/tool-calls?limit=${limit}`
    );
    return Array.isArray(data.tool_calls) ? data.tool_calls : [];
  }

  async getToolCall(callId: string): Promise<ToolCallRecord | null> {
    try {
      const data = await this.requestJson<ToolCallResponse>(
        `/api/observability/tool-calls/${encodeURIComponent(callId)}`
      );
      return data.tool_call ?? null;
    } catch (error) {
      if (error instanceof BorgApiError && error.status === 404) {
        return null;
      }
      throw error;
    }
  }
}

export function createBorgApiClient(
  options?: BorgApiClientOptions
): BorgApiClient {
  return new BorgApiClient(options);
}
