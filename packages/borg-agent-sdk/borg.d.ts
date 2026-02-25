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
   * Top-level Borg SDK surface available inside Code Mode execution.
   */
  interface BorgSdk {
    OS: BorgOS;
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
