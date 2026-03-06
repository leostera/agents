import {
  DeleteProviderDocument,
  ProviderByKeyDocument,
  type ProviderFieldsFragment,
  ProvidersListDocument,
  UpsertProviderDocument,
} from "./generated/operations";
import {
  GraphQLRequestError,
  requestGraphQLDocument,
  resolveDefaultBaseUrl,
} from "./runtime/client";

export type ProviderRecord = {
  provider: string;
  provider_kind: string;
  api_key: string;
  base_url?: string | null;
  enabled: boolean;
  tokens_used: number;
  last_used?: string | null;
  default_text_model?: string | null;
  default_audio_model?: string | null;
  created_at: string;
  updated_at: string;
};

export type UpsertProviderPayload = {
  provider: string;
  providerKind?: string | null;
  apiKey?: string | null;
  baseUrl?: string | null;
  enabled?: boolean;
  defaultTextModel?: string | null;
  defaultAudioModel?: string | null;
};

export type ProviderModelsResponse = {
  provider?: string;
  models?: string[];
  default_text_model?: string | null;
  default_audio_model?: string | null;
};

function mapProvider(node: ProviderFieldsFragment): ProviderRecord {
  return {
    provider: node.provider,
    provider_kind: node.providerKind,
    api_key: node.apiKey,
    base_url: node.baseUrl ?? null,
    enabled: node.enabled,
    tokens_used: node.tokensUsed,
    last_used: node.lastUsed ?? null,
    default_text_model: node.defaultTextModel ?? null,
    default_audio_model: node.defaultAudioModel ?? null,
    created_at: node.createdAt,
    updated_at: node.updatedAt,
  };
}

function resolveBaseUrl(baseUrl?: string): string {
  const resolved = (baseUrl ?? resolveDefaultBaseUrl()).replace(/\/+$/, "");
  return resolved;
}

async function requestJson<T>(
  path: string,
  init?: RequestInit,
  options: { baseUrl?: string } = {}
): Promise<T> {
  const response = await fetch(
    `${resolveBaseUrl(options.baseUrl)}${path}`,
    init
  );
  if (!response.ok) {
    const responseText = await response.text().catch(() => "");
    throw new GraphQLRequestError(`Request failed (${response.status})`, {
      status: response.status,
      responseText,
    });
  }
  const contentType = response.headers.get("content-type") ?? "";
  if (!contentType.includes("application/json")) {
    const responseText = await response.text().catch(() => "");
    throw new GraphQLRequestError("Endpoint returned non-JSON response", {
      status: response.status,
      responseText,
    });
  }
  return (await response.json()) as T;
}

async function request(path: string, init?: RequestInit): Promise<void> {
  const response = await fetch(`${resolveBaseUrl()}${path}`, init);
  if (!response.ok) {
    const responseText = await response.text().catch(() => "");
    throw new GraphQLRequestError(`Request failed (${response.status})`, {
      status: response.status,
      responseText,
    });
  }
}

export async function listProviders(first = 100): Promise<ProviderRecord[]> {
  const data = await requestGraphQLDocument(ProvidersListDocument, { first });
  const edges = data.providers.edges ?? [];
  return edges.map((edge) => mapProvider(edge.node));
}

export async function getProvider(
  provider: string
): Promise<ProviderRecord | null> {
  const data = await requestGraphQLDocument(ProviderByKeyDocument, {
    provider,
  });
  if (!data.provider) return null;
  return mapProvider(data.provider);
}

export async function upsertProvider(
  payload: UpsertProviderPayload
): Promise<ProviderRecord> {
  const data = await requestGraphQLDocument(UpsertProviderDocument, {
    input: {
      provider: payload.provider,
      providerKind: payload.providerKind ?? null,
      apiKey: payload.apiKey ?? null,
      baseUrl: payload.baseUrl ?? null,
      enabled: payload.enabled,
      defaultTextModel: payload.defaultTextModel ?? null,
      defaultAudioModel: payload.defaultAudioModel ?? null,
    },
  });
  return mapProvider(data.upsertProvider);
}

export async function deleteProvider(
  provider: string,
  options: { ignoreNotFound?: boolean } = {}
): Promise<void> {
  try {
    await requestGraphQLDocument(DeleteProviderDocument, { provider });
  } catch (error) {
    if (
      options.ignoreNotFound &&
      error instanceof GraphQLRequestError &&
      error.status === 404
    ) {
      return;
    }
    throw error;
  }
}

export async function getProviderModels(
  provider: string
): Promise<ProviderModelsResponse> {
  return requestJson<ProviderModelsResponse>(
    `/api/providers/${encodeURIComponent(provider)}/models`
  );
}

export async function listProviderModels(provider: string): Promise<string[]> {
  const data = await getProviderModels(provider);
  return Array.isArray(data.models) ? data.models : [];
}

export async function startOpenAiDeviceCode(): Promise<void> {
  await request("/api/providers/openai/device-code/start", {
    method: "POST",
  });
}
