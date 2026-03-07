import {
  ActorStatusValue,
  OnboardingUpsertActorDocument,
  OnboardingUpsertPortDocument,
} from "./generated/operations";
import { requestGraphQL, requestGraphQLDocument } from "./runtime/client";

export type UpsertOnboardingActorPayload = {
  actorId: string;
  name?: string | null;
  systemPrompt: string;
  status?: ActorStatusValue | null;
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
  actorId: string | null;
};

export type OnboardingActorMessage = {
  id: string;
  actorId: string;
  createdAt: string;
  messageType: string;
  role: string | null;
  text: string | null;
  payload: unknown;
};

export type OnboardingActorMessagesPage = {
  messages: OnboardingActorMessage[];
  endCursor: string | null;
  hasNextPage: boolean;
};

const ONBOARDING_PORT_BINDINGS_QUERY = `
  query OnboardingPortBindingsByPortId($portId: Uri!, $first: Int!) {
    portById(id: $portId) {
      bindings(first: $first) {
        edges {
          node {
            conversationKey
            actorId
          }
        }
      }
    }
  }
`;

const ONBOARDING_ACTOR_MESSAGES_QUERY = `
  query OnboardingActorMessages($actorId: Uri!, $first: Int!, $after: String) {
    actor(id: $actorId) {
      messages(first: $first, after: $after) {
        edges {
          node {
            id
            actorId
            createdAt
            messageType
            role
            text
            payload
          }
        }
        pageInfo {
          endCursor
          hasNextPage
        }
      }
    }
  }
`;

export async function upsertOnboardingActor(
  payload: UpsertOnboardingActorPayload
): Promise<void> {
  await requestGraphQLDocument(OnboardingUpsertActorDocument, {
    input: {
      id: payload.actorId,
      name: payload.name?.trim() || payload.actorId,
      systemPrompt: payload.systemPrompt,
      status: payload.status ?? ActorStatusValue.Running,
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
  type BindingData = {
    portById?: {
      bindings: {
        edges: Array<{
          node: {
            conversationKey: string;
            actorId?: string | null;
          };
        }>;
      };
    } | null;
  };

  const data = await requestGraphQL<
    BindingData,
    { portId: string; first: number }
  >({
    query: ONBOARDING_PORT_BINDINGS_QUERY,
    variables: { portId, first },
  });
  const edges = data.portById?.bindings.edges ?? [];
  return edges.map((edge) => ({
    conversationKey: edge.node.conversationKey,
    actorId: edge.node.actorId ?? null,
  }));
}

export async function listOnboardingActorMessages(
  actorId: string,
  first = 250
): Promise<OnboardingActorMessage[]> {
  const page = await listOnboardingActorMessagesPage(actorId, {
    first,
  });
  return page.messages;
}

export async function listOnboardingActorMessagesPage(
  actorId: string,
  options: {
    first?: number;
    after?: string | null;
  } = {}
): Promise<OnboardingActorMessagesPage> {
  type MessagesData = {
    actor?: {
      messages: {
        edges: Array<{
          node: {
            id: string;
            actorId: string;
            createdAt: string;
            messageType: string;
            role?: string | null;
            text?: string | null;
            payload: unknown;
          };
        }>;
        pageInfo: {
          endCursor?: string | null;
          hasNextPage: boolean;
        };
      };
    } | null;
  };

  const data = await requestGraphQL<
    MessagesData,
    { actorId: string; first: number; after: string | null }
  >({
    query: ONBOARDING_ACTOR_MESSAGES_QUERY,
    variables: {
      actorId,
      first: options.first ?? 250,
      after: options.after ?? null,
    },
  });
  const edges = data.actor?.messages.edges ?? [];
  return {
    messages: edges.map((edge) => ({
      id: edge.node.id,
      actorId: edge.node.actorId,
      createdAt: edge.node.createdAt,
      messageType: edge.node.messageType,
      role: edge.node.role ?? null,
      text: edge.node.text ?? null,
      payload: edge.node.payload,
    })),
    endCursor: data.actor?.messages.pageInfo.endCursor ?? null,
    hasNextPage: data.actor?.messages.pageInfo.hasNextPage ?? false,
  };
}
