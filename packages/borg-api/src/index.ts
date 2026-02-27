export type ProviderRecord = {
  provider: string;
  api_key: string;
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
  model: string;
  system_prompt: string;
  tools: unknown;
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
  provider: string;
  port: string;
  enabled: boolean;
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
};

export type LlmCallsResponse = {
  llm_calls?: LlmCallRecord[];
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

  async health(): Promise<boolean> {
    const data = await this.requestJson<HealthResponse>("/health");
    return data.status === "ok";
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
    model: string;
    systemPrompt: string;
    tools: unknown;
  }): Promise<void> {
    await this.request(
      `/api/agents/specs/${encodeURIComponent(payload.agentId)}`,
      {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          model: payload.model,
          system_prompt: payload.systemPrompt,
          tools: payload.tools,
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

  async listPortSettings(port: string, limit = 200): Promise<PortSetting[]> {
    const data = await this.requestJson<PortSettingsResponse>(
      `/api/ports/${encodeURIComponent(port)}/settings?limit=${limit}`
    );
    return Array.isArray(data.settings) ? data.settings : [];
  }

  async listPorts(limit = 200): Promise<PortRecord[]> {
    const data = await this.requestJson<PortsResponse>(
      `/api/ports?limit=${limit}`
    );
    return Array.isArray(data.ports) ? data.ports : [];
  }

  async deletePort(
    port: string,
    options: { ignoreNotFound?: boolean } = {}
  ): Promise<void> {
    try {
      await this.request(`/api/ports/${encodeURIComponent(port)}`, {
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

  async getPortSetting(port: string, key: string): Promise<string | null> {
    try {
      const data = await this.requestJson<PortSettingResponse>(
        `/api/ports/${encodeURIComponent(port)}/settings/${encodeURIComponent(key)}`
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
    port: string,
    key: string,
    value: string
  ): Promise<void> {
    await this.request(
      `/api/ports/${encodeURIComponent(port)}/settings/${encodeURIComponent(key)}`,
      {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ value }),
      }
    );
  }

  async deletePortSetting(
    port: string,
    key: string,
    options: { ignoreNotFound?: boolean } = {}
  ): Promise<void> {
    try {
      await this.request(
        `/api/ports/${encodeURIComponent(port)}/settings/${encodeURIComponent(key)}`,
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

  async listPortBindings(port: string, limit = 200): Promise<PortBinding[]> {
    const data = await this.requestJson<PortBindingsResponse>(
      `/api/ports/${encodeURIComponent(port)}/bindings?limit=${limit}`
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
}

export function createBorgApiClient(
  options?: BorgApiClientOptions
): BorgApiClient {
  return new BorgApiClient(options);
}
