import {
  createBorgApiClient,
  type ActorRecord,
  type PortRecord,
} from "@borg/api";
import {
  Badge,
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
import { Route } from "lucide-react";
import React from "react";
import {
  Section,
  SectionContent,
  SectionEmpty,
  SectionToolbar,
} from "../../../components/Section";
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

function modeChipClass(allowsGuests: boolean): string {
  return allowsGuests
    ? "border-yellow-300 bg-yellow-100 text-yellow-900"
    : "border-blue-300 bg-blue-100 text-blue-900";
}

function allowedUserIds(port: PortRecord): string[] {
  const provider = port.provider.trim().toLowerCase();
  if (provider !== "telegram" && provider !== "discord") return [];
  if (port.allows_guests) return [];
  const raw = port.settings?.allowed_external_user_ids;
  if (!Array.isArray(raw)) return [];
  return raw
    .map((value) => (typeof value === "string" ? value.trim() : ""))
    .filter(Boolean);
}

export function PortsPage() {
  const [ports, setPorts] = React.useState<PortRecord[]>([]);
  const [actors, setActors] = React.useState<ActorRecord[]>([]);
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
      const [loadedPorts, loadedActors] = await Promise.all([
        borgApi.listPorts(1000),
        borgApi.listActors(1000),
      ]);
      setPorts(loadedPorts);
      setActors(loadedActors);
    } catch (loadError) {
      setPorts([]);
      setActors([]);
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
  const hasAnyPorts = ports.length > 0;

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
      if (
        input.portKind === "discord" &&
        (input.discordBotToken ?? "").trim().length === 0
      ) {
        setError("Discord bot token is required.");
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
        if (input.portKind === "discord") {
          settings.bot_token = (input.discordBotToken ?? "").trim();
          settings.allowed_external_user_ids = [];
        }
        await borgApi.upsertPort(`borg:port:${port}`, {
          provider: input.portKind,
          enabled: true,
          allows_guests: true,
          assigned_actor_id: input.assignedActorId ?? null,
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

  const handleToggleEnabled = React.useCallback(
    async (port: PortRecord) => {
      setError(null);
      try {
        await borgApi.upsertPort(port.port_id, {
          provider: port.provider,
          enabled: !port.enabled,
          allows_guests: port.allows_guests,
          assigned_actor_id: port.assigned_actor_id ?? null,
          settings: port.settings,
        });
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
    <Section className="gap-4">
      {isLoading || hasAnyPorts ? (
        <SectionToolbar>
          <Input
            value={query}
            onChange={(event) => setQuery(event.currentTarget.value)}
            placeholder="Search ports"
            aria-label="Search ports"
          />
          <Button variant="outline" onClick={() => setIsDialogOpen(true)}>
            + Add Port
          </Button>
        </SectionToolbar>
      ) : null}

      {error ? <p className="text-destructive text-xs">{error}</p> : null}

      <SectionContent>
        {!isLoading && filteredPorts.length === 0 ? (
          <SectionEmpty
            icon={Route}
            title="No Ports Found"
            description="Create your first port to connect an external channel."
            action={
              <Button onClick={() => setIsDialogOpen(true)}>+ Add Port</Button>
            }
          />
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead className="w-8" />
                <TableHead>Provider</TableHead>
                <TableHead>Port Name</TableHead>
                <TableHead>Mode</TableHead>
                <TableHead>Allowed Users</TableHead>
                <TableHead>Active Sessions</TableHead>
                <TableHead>Updated</TableHead>
                <TableHead>Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {isLoading ? (
                <TableRow>
                  <TableCell
                    colSpan={8}
                    className="text-muted-foreground text-center"
                  >
                    Loading ports...
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
                    <TableCell>
                      <span
                        className={`inline-block h-2.5 w-2.5 rounded-full ${
                          port.enabled
                            ? "bg-green-500"
                            : "bg-muted-foreground/40"
                        }`}
                        title={port.enabled ? "Enabled" : "Disabled"}
                      />
                    </TableCell>
                    <TableCell>{formatProviderName(port.provider)}</TableCell>
                    <TableCell className="font-mono text-[11px]">
                      <Link
                        href={`/control/ports/${encodeURIComponent(port.port_id)}`}
                        onClick={(event) => event.stopPropagation()}
                      >
                        {port.port_name}
                      </Link>
                    </TableCell>
                    <TableCell>
                      <Badge
                        variant="outline"
                        className={modeChipClass(port.allows_guests)}
                      >
                        {port.allows_guests ? "Public" : "Private"}
                      </Badge>
                    </TableCell>
                    <TableCell>
                      <div className="flex flex-wrap items-center gap-1">
                        {allowedUserIds(port).map((userId) => (
                          <Badge key={userId} variant="outline">
                            {userId}
                          </Badge>
                        ))}
                      </div>
                    </TableCell>
                    <TableCell>{port.active_sessions}</TableCell>
                    <TableCell>{formatUpdatedAt(port.updated_at)}</TableCell>
                    <TableCell className="space-x-2">
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={(event) => {
                          event.stopPropagation();
                          void handleToggleEnabled(port);
                        }}
                      >
                        {port.enabled ? "Disable" : "Enable"}
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
        )}
      </SectionContent>

      <AddPortForm
        actors={actors}
        open={isDialogOpen}
        onOpenChange={setIsDialogOpen}
        isSaving={isSaving}
        onSubmit={handleCreatePort}
      />
    </Section>
  );
}
