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

const Borg = Object.freeze({
  OS,
  fetch: sdkFetch,
});

(globalThis as { Borg?: unknown }).Borg = Borg;

export {};
