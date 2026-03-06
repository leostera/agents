import type { TypedDocumentNode } from "@graphql-typed-document-node/core";
import { print } from "graphql";

export type GraphQLRequest<TVariables extends Record<string, unknown>> = {
  query: string;
  variables?: TVariables;
};

export type GraphQLErrorPayload = {
  message: string;
  path?: Array<string | number>;
  extensions?: Record<string, unknown>;
};

export type GraphQLResponse<TData> = {
  data?: TData;
  errors?: GraphQLErrorPayload[];
};

export class GraphQLRequestError extends Error {
  status?: number;
  responseText?: string;
  graphQLErrors?: GraphQLErrorPayload[];

  constructor(
    message: string,
    options: {
      status?: number;
      responseText?: string;
      graphQLErrors?: GraphQLErrorPayload[];
    } = {}
  ) {
    super(message);
    this.name = "GraphQLRequestError";
    this.status = options.status;
    this.responseText = options.responseText;
    this.graphQLErrors = options.graphQLErrors;
  }
}

export function resolveDefaultBaseUrl(): string {
  if (typeof window === "undefined") return "";
  const env = (
    import.meta as unknown as {
      env?: Record<string, string | undefined>;
    }
  ).env;
  const fromEnv = env?.BORG_API ?? env?.VITE_BORG_API_BASE_URL ?? "";
  if (fromEnv.trim()) return fromEnv.replace(/\/+$/, "");

  const { origin, hostname, protocol, port } = window.location;
  const isLocal = hostname === "localhost" || hostname === "127.0.0.1";
  if (isLocal && (port === "5173" || port === "4173")) {
    return `${protocol}//${hostname}:8080`;
  }
  return origin;
}

export async function requestGraphQL<
  TData,
  TVariables extends Record<string, unknown> = Record<string, unknown>,
>(
  input: GraphQLRequest<TVariables>,
  options: { baseUrl?: string } = {}
): Promise<TData> {
  const baseUrl = (options.baseUrl ?? resolveDefaultBaseUrl()).replace(
    /\/+$/,
    ""
  );
  const response = await fetch(`${baseUrl}/gql`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(input),
  });

  if (!response.ok) {
    const body = await response.text().catch(() => "");
    throw new GraphQLRequestError(
      `GraphQL request failed (${response.status})${body ? `: ${body}` : ""}`
    );
  }

  const payload = (await response.json()) as GraphQLResponse<TData>;
  if (Array.isArray(payload.errors) && payload.errors.length > 0) {
    const message = payload.errors.map((error) => error.message).join("; ");
    throw new GraphQLRequestError(
      message || "GraphQL request returned errors",
      { graphQLErrors: payload.errors }
    );
  }
  if (payload.data === undefined) {
    throw new GraphQLRequestError("GraphQL request returned no data");
  }
  return payload.data;
}

export async function requestGraphQLDocument<
  TData,
  TVariables extends Record<string, unknown> = Record<string, unknown>,
>(
  document: TypedDocumentNode<TData, TVariables>,
  variables?: TVariables,
  options: { baseUrl?: string } = {}
): Promise<TData> {
  return requestGraphQL<TData, TVariables>(
    {
      query: print(document),
      variables,
    },
    options
  );
}
