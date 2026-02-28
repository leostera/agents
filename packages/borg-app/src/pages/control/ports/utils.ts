import type { PortRecord } from "@borg/api";

export function toPortUri(port: PortRecord): string {
  return `borg:port:${port.port_name}`;
}

function extractPortNameFromUri(value: string): string | null {
  const trimmed = value.trim();
  if (!trimmed.startsWith("borg:port:")) return null;
  const name = trimmed.slice("borg:port:".length).trim();
  return name.length > 0 ? name : null;
}

export function resolvePortFromRoute(
  routeValue: string,
  ports: PortRecord[]
): PortRecord | null {
  const trimmed = routeValue.trim();
  if (!trimmed) return null;

  const byId = ports.find((port) => port.port_id === trimmed);
  if (byId) return byId;

  const fromUri = extractPortNameFromUri(trimmed);
  if (fromUri) {
    const byNameFromUri = ports.find((port) => port.port_name === fromUri);
    if (byNameFromUri) return byNameFromUri;
  }

  const byName = ports.find((port) => port.port_name === trimmed);
  if (byName) return byName;

  return null;
}
