type BorgUri = `${string}:${string}:${string}`;

type PathLike = string;

type BorgDirEntryKind = "file" | "directory" | "symlink" | "other";

interface BorgDirEntry {
  path: string;
  name: string;
  kind: BorgDirEntryKind;
}

interface BorgLsOptions {
  absolute?: boolean;
  recursive?: boolean;
  maxDepth?: number;
  includeHidden?: boolean;
  withFileTypes?: boolean;
}

interface BorgLsResult {
  cwd: string;
  basePath: string;
  entries: string[];
  detailedEntries: BorgDirEntry[];
}

interface BorgFetchInit {
  method?: string;
  headers?: Record<string, string>;
  body?: string | Record<string, unknown> | unknown[] | null;
  timeoutMs?: number;
}

interface BorgFetchResponse {
  ok: boolean;
  status: number;
  status_text: string;
  url: string;
  headers: Record<string, string>;
  body: string;
  json: unknown | null;
}

type BorgFactValue =
  | { Text: string }
  | { Integer: number }
  | { Float: number }
  | { Boolean: boolean }
  | { Bytes: number[] }
  | { Ref: BorgUri };

interface BorgFactInput {
  source?: BorgUri;
  entity: BorgUri;
  field: BorgUri;
  value: BorgFactValue;
}

interface BorgStateFactsResult {
  tx_id: BorgUri;
  facts: unknown[];
}

interface BorgNameFilter {
  like: string;
}

interface BorgSearchQuery {
  ns?: string;
  kind?: string;
  name?: BorgNameFilter;
  q?: string;
  limit?: number;
}

interface BorgSearchResults {
  entities: unknown[];
}

interface BorgOS {
  ls(path?: PathLike, options?: BorgLsOptions): BorgLsResult;
}

interface BorgMemory {
  stateFacts(facts: BorgFactInput[]): BorgStateFactsResult;
  search(query: BorgSearchQuery): BorgSearchResults;
}

interface BorgCurrentMessage {
  uri(): BorgUri | null;
}

interface BorgMessage {
  currentMessage(): BorgCurrentMessage;
}

interface BorgUser {
  uri(): BorgUri | null;
}

interface BorgSdk {
  OS: BorgOS;
  Memory: BorgMemory;
  Message: BorgMessage;
  me(): BorgUser;
  fetch(url: string, init?: BorgFetchInit): Promise<BorgFetchResponse>;
}

const ffiCall = (globalThis as { ffi?: (opName: string, args: unknown[]) => unknown }).ffi;

if (typeof ffiCall !== "function") {
  throw new Error("borg-agent-sdk requires global ffi(opName, args)");
}

function sdkFetch(...args: unknown[]): Promise<BorgFetchResponse> {
  const nativeFetch = (globalThis as { fetch?: (...fetchArgs: unknown[]) => unknown }).fetch;
  if (typeof nativeFetch === "function") {
    return Promise.resolve(nativeFetch(...args) as Promise<BorgFetchResponse>);
  }
  return Promise.resolve(ffiCall("net__fetch", args) as BorgFetchResponse);
}

function ffiInvoke<T>(opName: string, args: unknown[]): T {
  return ffiCall(opName, args) as T;
}

function currentContext(): Record<string, unknown> {
  return ffiInvoke<Record<string, unknown>>("context__current", []);
}

const OS: BorgOS = {
  ls(path?: PathLike, options?: BorgLsOptions): BorgLsResult {
    const args: unknown[] = [];
    if (path !== undefined) {
      args.push(path);
    }
    if (options !== undefined) {
      args.push(options);
    }
    return ffiInvoke<BorgLsResult>("os__ls", args);
  },
};

const Memory: BorgMemory = {
  stateFacts(facts: BorgFactInput[]): BorgStateFactsResult {
    return ffiInvoke<BorgStateFactsResult>("memory__state_facts", [facts]);
  },
  search(query: BorgSearchQuery): BorgSearchResults {
    return ffiInvoke<BorgSearchResults>("memory__search", [query]);
  },
};

const Message: BorgMessage = {
  currentMessage(): BorgCurrentMessage {
    const context = currentContext();
    const currentMessageId = context?.current_message_id;
    const uri =
      typeof currentMessageId === "string"
        ? (currentMessageId as BorgUri)
        : null;
    return Object.freeze({
      uri(): BorgUri | null {
        return uri;
      },
    });
  },
};

const Borg: BorgSdk = Object.freeze({
  OS,
  Memory,
  Message,
  me(): BorgUser {
    const context = currentContext();
    const currentUserId = context?.current_user_id;
    const uri =
      typeof currentUserId === "string" ? (currentUserId as BorgUri) : null;
    return Object.freeze({
      uri(): BorgUri | null {
        return uri;
      },
    });
  },
  fetch(url: string, init?: BorgFetchInit): Promise<BorgFetchResponse> {
    return sdkFetch(url, init);
  },
});

(globalThis as { Borg?: unknown }).Borg = Borg;

export {};
