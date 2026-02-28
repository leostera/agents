import { createBorgApiClient, type PortRecord } from "@borg/api";
import {
  Button,
  Input,
  Link,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@borg/ui";
import React from "react";
import { AddPortForm, type AddPortInput } from "./AddPortForm";

const borgApi = createBorgApiClient();

function normalize(value: string): string {
  return value.trim().toLowerCase();
}

function matchesTerm(port: PortRecord, term: string): boolean {
  if (!term) return true;
  return [port.port_name, port.provider].join(" ").toLowerCase().includes(term);
}

function formatUpdatedAt(value?: string | null): string {
  if (!value) return "—";
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) return "—";
  return parsed.toLocaleString();
}

function formatProviderName(provider?: string): string {
  const normalized = (provider ?? "custom").trim().toLowerCase();
  if (normalized === "telegram") return "Telegram";
  if (normalized === "whatsapp") return "WhatsApp";
  if (normalized === "discord") return "Discord";
  if (normalized === "x") return "X.com";
  if (normalized === "sms") return "SMS";
  if (normalized === "http") return "HTTP";
  if (normalized === "custom") return "Custom";
  return provider ?? "Custom";
}

export function PortsPage() {
  const [ports, setPorts] = React.useState<PortRecord[]>([]);
  const [isLoading, setIsLoading] = React.useState(true);
  const [error, setError] = React.useState<string | null>(null);
  const [query, setQuery] = React.useState(
    () => new URLSearchParams(window.location.search).get("q") ?? ""
  );
  const [isSaving, setIsSaving] = React.useState(false);
  const [isDialogOpen, setIsDialogOpen] = React.useState(false);

  const reload = React.useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const rows = await borgApi.listPorts(1000);
      setPorts(rows);
    } catch (loadError) {
      setPorts([]);
      setError(
        loadError instanceof Error ? loadError.message : "Unable to load ports"
      );
    } finally {
      setIsLoading(false);
    }
  }, []);

  React.useEffect(() => {
    void reload();
  }, [reload]);

  React.useEffect(() => {
    const params = new URLSearchParams();
    if (query.trim()) params.set("q", query.trim());
    const paramsString = params.toString();
    const nextUrl = paramsString
      ? `/control/ports?${paramsString}`
      : "/control/ports";
    window.history.replaceState(null, "", nextUrl);
  }, [query]);

  const filteredPorts = React.useMemo(() => {
    const term = normalize(query);
    return ports.filter((port) => matchesTerm(port, term));
  }, [ports, query]);

  const handleCreatePort = React.useCallback(
    async (input: AddPortInput) => {
      const port = input.portName.trim();
      if (!port) {
        setError("Port name is required.");
        return;
      }

      if (
        input.portKind === "telegram" &&
        (input.telegramBotToken ?? "").trim().length === 0
      ) {
        setError("Telegram bot token is required.");
        return;
      }

      setIsSaving(true);
      setError(null);
      try {
        const settings: Record<string, unknown> = {};
        if (input.portKind === "telegram") {
          settings.bot_token = (input.telegramBotToken ?? "").trim();
          settings.allowed_external_user_ids = [];
        }
        await borgApi.upsertPort(`borg:port:${port}`, {
          provider: input.portKind,
          enabled: true,
          allows_guests: true,
          settings,
        });
        setIsDialogOpen(false);
        await reload();
      } catch (saveError) {
        setError(
          saveError instanceof Error
            ? saveError.message
            : "Unable to create port configuration"
        );
      } finally {
        setIsSaving(false);
      }
    },
    [reload]
  );

  const handleTogglePause = React.useCallback(
    async (port: PortRecord) => {
      setError(null);
      try {
        await borgApi.upsertPortSetting(
          port.port_id,
          "enabled",
          port.enabled ? "false" : "true"
        );
        await reload();
      } catch (toggleError) {
        setError(
          toggleError instanceof Error
            ? toggleError.message
            : "Unable to update port state"
        );
      }
    },
    [reload]
  );

  const handleDeletePort = React.useCallback(
    async (port: PortRecord) => {
      const shouldDelete = window.confirm(
        `Delete port \"${port.port_name}\"? This removes its settings and bindings.`
      );
      if (!shouldDelete) return;

      setError(null);
      try {
        await borgApi.deletePort(port.port_id, { ignoreNotFound: true });
        await reload();
      } catch (deleteError) {
        setError(
          deleteError instanceof Error
            ? deleteError.message
            : "Unable to delete port"
        );
      }
    },
    [reload]
  );

  return (
    <section className="space-y-4">
      <section className="flex items-center gap-2">
        <Input
          value={query}
          onChange={(event) => setQuery(event.currentTarget.value)}
          placeholder="Search ports"
          aria-label="Search ports"
        />
        <Button variant="outline" onClick={() => setIsDialogOpen(true)}>
          + Add Port
        </Button>
      </section>

      {error ? <p className="text-destructive text-xs">{error}</p> : null}

      <section>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Provider</TableHead>
              <TableHead>Port Name</TableHead>
              <TableHead>Active Sessions</TableHead>
              <TableHead>Updated</TableHead>
              <TableHead>Actions</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {isLoading ? (
              <TableRow>
                <TableCell
                  colSpan={5}
                  className="text-muted-foreground text-center"
                >
                  Loading ports...
                </TableCell>
              </TableRow>
            ) : filteredPorts.length === 0 ? (
              <TableRow>
                <TableCell
                  colSpan={5}
                  className="text-muted-foreground text-center"
                >
                  No ports found.
                </TableCell>
              </TableRow>
            ) : (
              filteredPorts.map((port) => (
                <TableRow
                  key={port.port_id}
                  className="cursor-pointer"
                  onClick={() => {
                    window.history.pushState(
                      null,
                      "",
                      `/control/ports/${encodeURIComponent(port.port_id)}`
                    );
                    window.dispatchEvent(new PopStateEvent("popstate"));
                  }}
                >
                  <TableCell>{formatProviderName(port.provider)}</TableCell>
                  <TableCell className="font-mono text-[11px]">
                    <Link
                      href={`/control/ports/${encodeURIComponent(port.port_id)}`}
                      onClick={(event) => event.stopPropagation()}
                    >
                      {port.port_name}
                    </Link>
                  </TableCell>
                  <TableCell>{port.active_sessions}</TableCell>
                  <TableCell>{formatUpdatedAt(port.updated_at)}</TableCell>
                  <TableCell className="space-x-2">
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={(event) => {
                        event.stopPropagation();
                        void handleTogglePause(port);
                      }}
                    >
                      {port.enabled ? "Pause" : "Resume"}
                    </Button>
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={(event) => {
                        event.stopPropagation();
                        window.history.pushState(
                          null,
                          "",
                          `/control/ports/${encodeURIComponent(port.port_id)}`
                        );
                        window.dispatchEvent(new PopStateEvent("popstate"));
                      }}
                    >
                      Edit
                    </Button>
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={(event) => {
                        event.stopPropagation();
                        void handleDeletePort(port);
                      }}
                    >
                      Delete
                    </Button>
                  </TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </Table>
      </section>

      <AddPortForm
        open={isDialogOpen}
        onOpenChange={setIsDialogOpen}
        isSaving={isSaving}
        onSubmit={handleCreatePort}
      />
    </section>
  );
}
