declare global {
  type FfiCall = (opName: string, args: unknown[]) => unknown;

  const ffi: FfiCall;

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

  interface BorgOS {
    ls(path?: PathLike, options?: BorgLsOptions): BorgLsResult;
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

  interface BorgSdk {
    OS: BorgOS;
    fetch: (url: string, init?: BorgFetchInit) => BorgFetchResponse;
  }

  const Borg: BorgSdk;
}

export {};
