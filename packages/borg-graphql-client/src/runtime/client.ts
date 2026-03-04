export type GraphQLRequest = {
  query: string;
  variables?: Record<string, unknown>;
};

export type GraphQLResponse<TData> = {
  data?: TData;
  errors?: Array<{ message: string }>;
};

function resolveDefaultBaseUrl(): string {
  if (typeof window === "undefined") return "";
  const fromEnv =
    (import.meta as unknown as { env?: Record<string, string | undefined> }).env
      ?.VITE_BORG_API_BASE_URL ?? "";
  if (fromEnv.trim()) return fromEnv.replace(/\/+$/, "");

  const { origin, hostname, protocol, port } = window.location;
  const isLocal = hostname === "localhost" || hostname === "127.0.0.1";
  if (isLocal && (port === "5173" || port === "4173")) {
    return `${protocol}//${hostname}:8080`;
  }
  return origin;
}

export async function requestGraphQL<TData>(
  input: GraphQLRequest,
  options: { baseUrl?: string } = {}
): Promise<GraphQLResponse<TData>> {
  const baseUrl = (options.baseUrl ?? resolveDefaultBaseUrl()).replace(
    /\/+$/,
    ""
  );
  const response = await fetch(`${baseUrl}/graphql`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(input),
  });

  if (!response.ok) {
    const body = await response.text().catch(() => "");
    throw new Error(
      `GraphQL request failed (${response.status})${body ? `: ${body}` : ""}`
    );
  }

  return (await response.json()) as GraphQLResponse<TData>;
}
