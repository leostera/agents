const ffiCall = (globalThis as { ffi?: (opName: string, args: unknown[]) => unknown }).ffi;

if (typeof ffiCall !== "function") {
  throw new Error("borg-agent-sdk requires global ffi(opName, args)");
}

function sdkFetch(...args: unknown[]): unknown {
  const nativeFetch = (globalThis as { fetch?: (...fetchArgs: unknown[]) => unknown }).fetch;
  if (typeof nativeFetch === "function") {
    return nativeFetch(...args);
  }
  return ffiCall("net__fetch", args);
}

const OS = {
  ls: (...args: unknown[]) => ffiCall("os__ls", args),
};

const Memory = {
  stateFacts: (...args: unknown[]) => ffiCall("memory__state_facts", args),
  search: (...args: unknown[]) => ffiCall("memory__search", args),
};

const URI = {
  new: (ns: string, kind: string, id?: string) => {
    if (!ns || !kind) {
      throw new Error("Borg.URI.new requires non-empty ns and kind");
    }
    const value =
      typeof id === "string" && id.length > 0
        ? id
        : typeof crypto !== "undefined" && typeof crypto.randomUUID === "function"
          ? crypto.randomUUID()
          : `${Date.now()}-${Math.random().toString(16).slice(2)}`;
    return `${ns}:${kind}:${value}`;
  },
  parse: (raw: string) => {
    if (typeof raw !== "string") {
      throw new Error("Borg.URI.parse requires a string");
    }
    const value = raw.trim();
    const parts = value.split(":");
    if (parts.length !== 3 || parts.some((part) => part.length === 0)) {
      throw new Error(`Invalid Borg URI: ${raw}`);
    }
    return `${parts[0]}:${parts[1]}:${parts[2]}`;
  },
};

const Borg = Object.freeze({
  OS,
  Memory,
  URI,
  fetch: sdkFetch,
});

(globalThis as { Borg?: unknown }).Borg = Borg;

export {};
