declare global {
  /**
   * Low-level FFI bridge exposed by the runtime.
   * SDK consumers should prefer the typed `Borg` APIs instead of calling this directly.
   */
  type FfiCall = (opName: string, args: unknown[]) => unknown;

  const ffi: FfiCall;

  /**
   * String path understood by runtime file APIs.
   * Use absolute paths when possible to avoid ambiguity.
   */
  type PathLike = string;

  /**
   * Kind classification for a directory entry.
   */
  type BorgDirEntryKind = "file" | "directory" | "symlink" | "other";

  /**
   * Structured metadata for one filesystem entry returned by `Borg.OS.ls`.
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
    /** Return absolute paths in results when true. */
    absolute?: boolean;
    /** Traverse child directories recursively when true. */
    recursive?: boolean;
    /** Maximum recursive depth (only applies when `recursive` is true). */
    maxDepth?: number;
    /** Include hidden files/directories when true. */
    includeHidden?: boolean;
    /** Include typed `detailedEntries` output. */
    withFileTypes?: boolean;
  }

  /**
   * Result payload for directory listing.
   *
   * - `entries`: simple list of paths (useful for prompts and quick scans).
   * - `detailedEntries`: richer typed entries when `withFileTypes` is enabled.
   */
  interface BorgLsResult {
    cwd: string;
    basePath: string;
    entries: string[];
    detailedEntries: BorgDirEntry[];
  }

  /**
   * Operating-system helpers exposed by Borg.
   */
  interface BorgOS {
    /**
     * List files and directories under a path.
     *
     * Typical usage:
     * - `Borg.OS.ls()` to list current working directory.
     * - `Borg.OS.ls("/tmp", { withFileTypes: true })` for typed metadata.
     */
    ls(path?: PathLike, options?: BorgLsOptions): BorgLsResult;
  }

  /**
   * Request options for `Borg.fetch`.
   * Closely follows a simplified fetch-like shape.
   */
  interface BorgFetchInit {
    method?: string;
    headers?: Record<string, string>;
    body?: string | Record<string, unknown> | unknown[] | null;
    timeoutMs?: number;
  }

  /**
   * Normalized response from `Borg.fetch`.
   *
   * - `body`: raw response text.
   * - `json`: parsed JSON when body is valid JSON, otherwise `null`.
   */
  interface BorgFetchResponse {
    ok: boolean;
    status: number;
    status_text: string;
    url: string;
    headers: Record<string, string>;
    body: string;
    json: unknown | null;
  }

  /**
   * Input fact payload for `Borg.Memory.stateFacts`.
   *
   * All URI-like fields must be canonical URI strings (for example `borg:source:cli`).
   * `value` follows the tagged Rust enum shape expected by the backend.
   */
  interface BorgFactInput {
    source: string;
    entity: string;
    field: string;
    value:
      | { Text: string }
      | { Integer: number }
      | { Float: number }
      | { Boolean: boolean }
      | { Bytes: number[] }
      | { Ref: string };
  }

  /**
   * Result payload returned by `Borg.Memory.stateFacts`.
   *
   * - `tx_id`: transaction URI for this state operation.
   * - `facts`: persisted fact records (opaque but serializable).
   */
  interface BorgStateFactsResult {
    tx_id: string;
    facts: unknown[];
  }

  /**
   * Partial name filter used by memory search query.
   */
  interface BorgNameFilter {
    like: string;
  }

  /**
   * Query payload for `Borg.Memory.search`.
   *
   * `q` and/or `name.like` can be used for text matching.
   */
  interface BorgSearchQuery {
    ns?: string;
    kind?: string;
    name?: BorgNameFilter;
    q?: string;
    limit?: number;
  }

  /**
   * Result payload returned by `Borg.Memory.search`.
   */
  interface BorgSearchResults {
    entities: unknown[];
  }

  /**
   * Long-term memory APIs exposed by Borg.
   */
  interface BorgMemory {
    /**
     * Persist one or more facts into the long-term memory store.
     *
     * `source`, `entity`, and `field` must be URI strings.
     * Prefer using `Borg.URI.new(ns, kind)` for new identifiers and
     * `Borg.URI.parse(raw)` when normalizing existing raw strings.
     *
     * Fact modeling guidelines:
     * - Prefer many small atomic facts over one large compound fact.
     * - Keep one relationship per fact row.
     * - Reuse stable entity URIs across facts (do not create duplicates for the same person/object).
     * - Use `{ Ref: "<uri>" }` when linking one entity to another entity URI.
     * - Use scalar values (`Text`, `Integer`, `Boolean`, etc.) for primitive attributes.
     * - For `source`, prefer the most specific provenance URI available:
     *   1) `borg:message:<uuid>` (best),
     *   2) `borg:session:<id>`,
     *   3) `borg:user:<id>` (fallback).
     *
     * Example (good, granular):
     * `Borg.Memory.stateFacts([
     *   { source: Borg.URI.parse("borg:message:019c95d2-5757-7f90-85b6-67875fa81a7f"), entity: Borg.URI.parse("borg:user:leostera"), field: Borg.URI.parse("borg:field:telegram_id"), value: { Text: "2654566" } },
     *   { source: Borg.URI.parse("borg:message:019c95d2-5757-7f90-85b6-67875fa81a7f"), entity: Borg.URI.parse("borg:user:mariana_zabrodska"), field: Borg.URI.parse("borg:field:telegram_id"), value: { Text: "123456789" } },
     *   { source: Borg.URI.parse("borg:message:019c95d2-5757-7f90-85b6-67875fa81a7f"), entity: Borg.URI.parse("borg:user:leostera"), field: Borg.URI.parse("borg:relationship:girlfriend"), value: { Ref: Borg.URI.parse("borg:user:mariana_zabrodska") } },
     *   { source: Borg.URI.parse("borg:message:019c95d2-5757-7f90-85b6-67875fa81a7f"), entity: Borg.URI.parse("borg:user:mariana_zabrodska"), field: Borg.URI.parse("borg:relationship:boyfriend"), value: { Ref: Borg.URI.parse("borg:user:leostera") } }
     * ])`
     */
    stateFacts(facts: BorgFactInput[]): BorgStateFactsResult;
    /**
     * Search long-term memory entities.
     *
     * Example:
     * `Borg.Memory.search({ q: "movie", kind: "Preference", limit: 10 })`
     */
    search(query: BorgSearchQuery): BorgSearchResults;
  }

  /**
   * URI helpers for constructing and validating Borg URI strings.
   */
  interface BorgURI {
    /**
     * Create a new URI in the form `${ns}:${kind}:${id}`.
     * If `id` is omitted, a new random id is generated.
     */
    new(ns: string, kind: string, id?: string): string;
    /**
     * Validate and normalize an existing URI string.
     * Throws when the input is not a valid `ns:kind:id` URI.
     */
    parse(raw: string): string;
  }

  /**
   * Top-level Borg SDK surface available inside Code Mode execution.
   */
  interface BorgSdk {
    OS: BorgOS;
    /**
     * Long-term memory namespace for storing and searching structured facts.
     */
    Memory: BorgMemory;
    /**
     * URI helper namespace.
     */
    URI: BorgURI;
    /**
     * Perform an HTTP request from the runtime.
     *
     * Typical usage:
     * - `Borg.fetch("https://example.com/api")`
     * - `Borg.fetch(url, { method: "POST", headers: {...}, body: {...} })`
     */
    fetch: (url: string, init?: BorgFetchInit) => BorgFetchResponse;
  }

  const Borg: BorgSdk;
}

export {};
