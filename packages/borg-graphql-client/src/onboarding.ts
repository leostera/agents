import {
  OnboardingPortBindingsByPortIdDocument,
  OnboardingSessionMessagesDocument,
  OnboardingUpsertActorDocument,
  OnboardingUpsertPortDocument,
} from "./generated/operations";
import { requestGraphQLDocument } from "./runtime/client";

export type UpsertOnboardingActorPayload = {
  actorId: string;
  name?: string | null;
  systemPrompt: string;
  status?: string | null;
};

export type UpsertOnboardingPortPayload = {
  provider: string;
  enabled: boolean;
  allows_guests: boolean;
  assigned_actor_id?: string | null;
  settings?: Record<string, unknown>;
};

export type OnboardingPortBinding = {
  conversationKey: string;
  sessionId: string;
  actorId: string | null;
};

export type OnboardingSessionMessage = {
  id: string;
  sessionId: string;
  createdAt: string;
  messageType: string;
  role: string | null;
  text: string | null;
  payload: unknown;
};

export type OnboardingSessionMessagesPage = {
  messages: OnboardingSessionMessage[];
  endCursor: string | null;
  hasNextPage: boolean;
};

export async function upsertOnboardingActor(
  payload: UpsertOnboardingActorPayload
): Promise<void> {
  await requestGraphQLDocument(OnboardingUpsertActorDocument, {
    input: {
      id: payload.actorId,
      name: payload.name?.trim() || payload.actorId,
      systemPrompt: payload.systemPrompt,
      status: payload.status ?? "RUNNING",
    },
  });
}

export async function upsertOnboardingPort(
  portId: string,
  payload: UpsertOnboardingPortPayload
): Promise<void> {
  await requestGraphQLDocument(OnboardingUpsertPortDocument, {
    input: {
      name: portId,
      provider: payload.provider,
      enabled: payload.enabled,
      allowsGuests: payload.allows_guests,
      assignedActorId: payload.assigned_actor_id ?? null,
      settings: payload.settings ?? {},
    },
  });
}

export async function listOnboardingPortBindingsByPortId(
  portId: string,
  first = 50
): Promise<OnboardingPortBinding[]> {
  const data = await requestGraphQLDocument(
    OnboardingPortBindingsByPortIdDocument,
    {
      portId,
      first,
    }
  );
  const edges = data.portById?.bindings.edges ?? [];
  return edges.map((edge) => ({
    conversationKey: edge.node.conversationKey,
    sessionId: edge.node.sessionId,
    actorId: edge.node.actor?.id ?? null,
  }));
}

export async function listOnboardingSessionMessages(
  sessionId: string,
  first = 250
): Promise<OnboardingSessionMessage[]> {
  const page = await listOnboardingSessionMessagesPage(sessionId, {
    first,
  });
  return page.messages;
}

export async function listOnboardingSessionMessagesPage(
  sessionId: string,
  options: {
    first?: number;
    after?: string | null;
  } = {}
): Promise<OnboardingSessionMessagesPage> {
  const data = await requestGraphQLDocument(OnboardingSessionMessagesDocument, {
    sessionId,
    first: options.first ?? 250,
    after: options.after ?? null,
  });
  const edges = data.session?.messages.edges ?? [];
  return {
    messages: edges.map((edge) => ({
      id: edge.node.id,
      sessionId: edge.node.sessionId,
      createdAt: edge.node.createdAt,
      messageType: edge.node.messageType,
      role: edge.node.role ?? null,
      text: edge.node.text ?? null,
      payload: edge.node.payload,
    })),
    endCursor: data.session?.messages.pageInfo.endCursor ?? null,
    hasNextPage: data.session?.messages.pageInfo.hasNextPage ?? false,
  };
}
