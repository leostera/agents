declare global {
  type FfiCall = (opName: string, args: unknown[]) => unknown;

  const ffi: FfiCall;

  interface BorgOS {
    ls(...args: string[]): {
      entries: string[];
    };
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
    fetch: (url: string, init?: Record<string, unknown>) => BorgFetchResponse;
  }

  const Borg: BorgSdk;
}

export {};
