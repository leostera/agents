import { ActorStatusValue } from "@borg/graphql-client";

export type RuntimeStatus = "checking" | "online" | "offline";

export type ActorSummary = {
  id: string;
  name: string;
  systemPrompt: string;
  provider: string;
  model: string;
  status: string;
  createdAt: string;
  updatedAt: string;
};

export type MailboxMessage = {
  id: string;
  createdAt: string;
  messageType: string;
  role: string | null;
  text: string | null;
  payload: unknown;
};

export type ActorMailbox = {
  actorId: string;
  actorName: string;
  actorStatus: string;
  messages: MailboxMessage[];
};

export type CreateActorDraft = {
  actorId: string;
  name: string;
  provider: string;
  model: string;
  status: ActorStatusValue;
  systemPrompt: string;
};

export type ProviderInfo = {
  id: string;
  provider: string;
  providerKind: string;
  enabled: boolean;
  tokensUsed: number;
  baseUrl: string | null;
  defaultTextModel: string | null;
  defaultModel: string | null;
  models: string[];
};

export type PortSummary = {
  id: string;
  name: string;
  provider: string;
  enabled: boolean;
  allowsGuests: boolean;
  assignedActorId: string | null;
  settings: unknown;
  bindings: Array<{
    id: string;
    conversationKey: string;
    actorId: string;
  }>;
  actorBindings: Array<{
    id: string;
    conversationKey: string;
    actorId: string | null;
  }>;
  actorIds: string[];
};

export type ActorTab = "details" | "mailbox" | "context";

export type ActorDetailsDraft = {
  name: string;
  provider: string;
  model: string;
  status: ActorStatusValue;
  systemPrompt: string;
};

export type PortDetailsDraft = {
  name: string;
  provider: string;
  enabled: boolean;
  allowsGuests: boolean;
  assignedActorId: string;
  settings: string;
};

export type MailboxEntry =
  | {
      kind: "message";
      key: string;
      message: MailboxMessage;
    }
  | {
      kind: "tool";
      key: string;
      role: string;
      createdAt: string;
      toolName: string;
      fields: Array<{ key: string; value: string }>;
      sourceType: "tool_call" | "tool_result";
    };

export type ToolMailboxEntry = Extract<MailboxEntry, { kind: "tool" }>;

export type ActorContextWindow = {
  systemPrompt: string;
  behaviorPrompt: string;
  availableTools: Array<{
    name: string;
    description: string;
    parameters: any;
  }>;
  availableCapabilities: Array<{
    name: string;
    description: string;
  }>;
  orderedMessages: Array<{
    type: string;
    content: string;
    role?: string | null;
    toolCalls?: any[] | null;
  }>;
};

export type StageActorsResponse = {
  actors: {
    edges: Array<{
      node: {
        id: string;
        name: string;
        model: string | null;
        systemPrompt: string;
        status: string;
        createdAt: string;
        updatedAt: string;
      };
    }>;
  };
};

export type StageActorMailboxResponse = {
  actor: {
    id: string;
    name: string;
    status: string;
    messages: {
      edges: Array<{
        node: {
          id: string;
          createdAt: string;
          messageType: string;
          role: string | null;
          text: string | null;
          payload: unknown;
        };
      }>;
    };
  } | null;
};

export type StageUpsertActorResponse = {
  upsertActor: {
    id: string;
    name: string;
    status: string;
  };
};

export type StageDeleteActorResponse = {
  deleteActor: boolean;
};

export type StageUpsertPortResponse = {
  upsertPort: {
    id: string;
    name: string;
    provider: string;
    enabled: boolean;
    allowsGuests: boolean;
    assignedActorId: string | null;
    settings: unknown;
  };
};

export type StageUpsertPortActorBindingResponse = {
  upsertPortActorBinding: {
    id: string;
    conversationKey: string;
    actorId: string | null;
  };
};

export type StageProvidersResponse = {
  providers: {
    edges: Array<{
      node: {
        id: string;
        provider: string;
        providerKind: string;
        enabled: boolean;
        defaultTextModel: string | null;
        defaultModel: {
          name: string;
        } | null;
        models: Array<{
          name: string;
        }>;
      };
    }>;
  };
};

export type StagePortsResponse = {
  ports: {
    edges: Array<{
      node: {
        id: string;
        name: string;
        provider: string;
        enabled: boolean;
        allowsGuests: boolean;
        assignedActorId: string | null;
        settings: unknown;
        bindings: {
          edges: Array<{
            node: {
              id: string;
              conversationKey: string;
              actorId: string;
            };
          }>;
        };
        actorBindings: {
          edges: Array<{
            node: {
              id: string;
              conversationKey: string;
              actorId: string | null;
            };
          }>;
        };
      };
    }>;
  };
};
