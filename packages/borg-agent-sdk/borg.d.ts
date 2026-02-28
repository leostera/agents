/* eslint-disable */

/**
 * Canonical Borg URI string in `ns:kind:id` form.
 *
 * Example:
 * - `borg:user:leostera`
 * - `borg:message:019c95d2-5757-7f90-85b6-67875fa81a7f`
 */
type BorgUri = `${string}:${string}:${string}`;

/**
 * Path accepted by Borg filesystem APIs.
 *
 * Example:
 * - `"."`
 * - `"/tmp"`
 * - `"/Users/leostera/Movies"`
 */
type PathLike = string;

/**
 * Type classification for one filesystem entry.
 */
type BorgDirEntryKind = "file" | "directory" | "symlink" | "other";

/**
 * Rich metadata for a single file or directory entry.
 */
interface BorgDirEntry {
  path: string;
  name: string;
  kind: BorgDirEntryKind;
}

/**
 * Options for directory listing behavior.
 */
interface BorgLsOptions {
  /** Return absolute paths when true. */
  absolute?: boolean;
  /** Recursively traverse directories when true. */
  recursive?: boolean;
  /** Max traversal depth (only when recursive=true). */
  maxDepth?: number;
  /** Include hidden files/directories (dotfiles) when true. */
  includeHidden?: boolean;
  /** Include `detailedEntries` metadata when true. */
  withFileTypes?: boolean;
}

/**
 * Result payload from `Borg.OS.ls(...)`.
 */
interface BorgLsResult {
  /** Current working directory seen by the runtime. */
  cwd: string;
  /** Base path that was listed. */
  basePath: string;
  /** Path-only entries, useful for quick listing. */
  entries: string[];
  /** Typed entries, populated when `withFileTypes` is true. */
  detailedEntries: BorgDirEntry[];
}

/**
 * Options for `Borg.fetch(...)`.
 */
interface BorgFetchInit {
  /** HTTP method, defaults to `GET`. */
  method?: string;
  /** Request headers. */
  headers?: Record<string, string>;
  /** Request body string or JSON-like object/array. */
  body?: string | Record<string, unknown> | unknown[] | null;
  /** Optional timeout in milliseconds. */
  timeoutMs?: number;
}

/**
 * Normalized response payload for `Borg.fetch(...)`.
 */
interface BorgFetchResponse {
  /** True when status is 2xx. */
  ok: boolean;
  /** HTTP status code. */
  status: number;
  /** HTTP status text. */
  status_text: string;
  /** Final request URL. */
  url: string;
  /** Response headers as a plain object. */
  headers: Record<string, string>;
  /** Raw response body text. */
  body: string;
  /** Parsed JSON body when possible, otherwise null. */
  json: unknown | null;
}

interface BorgOS {
  /**
   * List files and directories under a path.
   *
   * Example:
   * ```ts
   * const listing = Borg.OS.ls(".")
   * ```
   *
   * Example:
   * ```ts
   * const deep = Borg.OS.ls("/tmp", { recursive: true, maxDepth: 2, withFileTypes: true })
   * ```
   */
  ls(path?: PathLike, options?: BorgLsOptions): BorgLsResult;
}

/**
 * Message-scoped context for the currently executing turn.
 */
interface BorgCurrentMessage {
  /**
   * URI for the current inbound message, when available.
   *
   * Example:
   * ```ts
   * const messageId = Borg.Message.currentMessage().uri()
   * ```
   */
  uri(): BorgUri | null;
}

interface BorgMessage {
  /**
   * Access the current inbound message context.
   */
  currentMessage(): BorgCurrentMessage;
}

/**
 * User-scoped context for the currently executing turn.
 */
interface BorgUser {
  /**
   * URI for the current user, when available.
   *
   * Example:
   * ```ts
   * const userId = Borg.me().uri()
   * ```
   */
  uri(): BorgUri | null;
}

/**
 * Root Borg SDK available in Code Mode runtime.
 */
interface BorgSdk {
  /** Operating-system helpers. */
  OS: BorgOS;
  /** Current message context helpers. */
  Message: BorgMessage;
  /**
   * Access the current user context.
   *
   * Example:
   * ```ts
   * const me = Borg.me().uri()
   * if (me) console.log(me)
   * ```
   */
  me(): BorgUser;
  /**
   * Perform an HTTP request.
   *
   * Example:
   * ```ts
   * const res = await Borg.fetch("https://example.com/api")
   * if (res.ok) console.log(res.json ?? res.body)
   * ```
   */
  fetch(url: string, init?: BorgFetchInit): Promise<BorgFetchResponse>;
}

/**
 * Global Borg API object exposed by the runtime.
 */
declare const Borg: BorgSdk;

export {};
