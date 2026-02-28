import {
  createBorgApiClient,
  type PortBinding,
  type PortSetting,
  type SessionRecord,
} from "@borg/api";
import {
  Badge,
  Button,
  Input,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@borg/ui";
import React from "react";

const borgApi = createBorgApiClient();

type PortDetailsPageProps = {
  portUri: string;
};

export function PortDetailsPage({ portUri }: PortDetailsPageProps) {
  const [settings, setSettings] = React.useState<PortSetting[]>([]);
  const [bindings, setBindings] = React.useState<PortBinding[]>([]);
  const [sessions, setSessions] = React.useState<SessionRecord[]>([]);
  const [isLoading, setIsLoading] = React.useState(true);
  const [error, setError] = React.useState<string | null>(null);
  const [keyInput, setKeyInput] = React.useState("");
  const [valueInput, setValueInput] = React.useState("");
  const [isSaving, setIsSaving] = React.useState(false);

  const load = React.useCallback(async () => {
    const normalizedPortUri = portUri.trim();
    if (!normalizedPortUri) {
      setError("Missing port name");
      setSettings([]);
      setBindings([]);
      setSessions([]);
      setIsLoading(false);
      return;
    }

    setIsLoading(true);
    setError(null);
    try {
      const [loadedSettings, loadedBindings, loadedSessions] =
        await Promise.all([
          borgApi.listPortSettings(normalizedPortUri, 1000),
          borgApi.listPortBindings(normalizedPortUri, 1000),
          borgApi.listSessions(10000),
        ]);
      setSettings(loadedSettings);
      setBindings(loadedBindings);
      setSessions(
        loadedSessions.filter((session) => session.port === normalizedPortUri)
      );
    } catch (loadError) {
      setSettings([]);
      setBindings([]);
      setSessions([]);
      setError(
        loadError instanceof Error
          ? loadError.message
          : "Unable to load port details"
      );
    } finally {
      setIsLoading(false);
    }
  }, [portUri]);

  React.useEffect(() => {
    void load();
  }, [load]);

  const handleSaveSetting = React.useCallback(
    async (event: React.FormEvent<HTMLFormElement>) => {
      event.preventDefault();
      const normalizedPortUri = portUri.trim();
      const key = keyInput.trim();
      if (!normalizedPortUri || !key) {
        setError("Port and key are required.");
        return;
      }

      setIsSaving(true);
      setError(null);
      try {
        await borgApi.upsertPortSetting(normalizedPortUri, key, valueInput);
        setKeyInput("");
        setValueInput("");
        await load();
      } catch (saveError) {
        setError(
          saveError instanceof Error
            ? saveError.message
            : "Unable to save setting"
        );
      } finally {
        setIsSaving(false);
      }
    },
    [keyInput, load, portUri, valueInput]
  );

  const handleEditSetting = React.useCallback((setting: PortSetting) => {
    setKeyInput(setting.key);
    setValueInput(setting.value);
  }, []);

  const handleDeleteSetting = React.useCallback(
    async (key: string) => {
      setError(null);
      try {
        await borgApi.deletePortSetting(portUri, key, { ignoreNotFound: true });
        await load();
      } catch (deleteError) {
        setError(
          deleteError instanceof Error
            ? deleteError.message
            : "Unable to delete setting"
        );
      }
    },
    [load, portUri]
  );

  if (isLoading) {
    return <p className="text-muted-foreground text-sm">Loading port...</p>;
  }

  if (error) {
    return <p className="text-destructive text-sm">{error}</p>;
  }

  return (
    <section className="space-y-6">
      <section className="grid gap-3 md:grid-cols-3">
        <div>
          <p className="text-muted-foreground text-xs">Port</p>
          <p className="font-mono text-xs break-all">{portUri}</p>
        </div>
        <div>
          <p className="text-muted-foreground text-xs">Settings</p>
          <Badge variant="outline">{settings.length}</Badge>
        </div>
        <div>
          <p className="text-muted-foreground text-xs">Active Sessions</p>
          <Badge variant="outline">{sessions.length}</Badge>
        </div>
      </section>

      <section className="space-y-2">
        <p className="text-sm font-semibold">Configure Port</p>
        <form
          className="grid gap-2 md:grid-cols-3"
          onSubmit={handleSaveSetting}
        >
          <Input
            value={keyInput}
            onChange={(event) => setKeyInput(event.currentTarget.value)}
            placeholder="Key"
            aria-label="Setting key"
          />
          <Input
            value={valueInput}
            onChange={(event) => setValueInput(event.currentTarget.value)}
            placeholder="Value"
            aria-label="Setting value"
          />
          <Button type="submit" disabled={isSaving}>
            {isSaving ? "Saving..." : "Save setting"}
          </Button>
        </form>
      </section>

      <section className="space-y-2">
        <p className="text-sm font-semibold">Settings</p>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Key</TableHead>
              <TableHead>Value</TableHead>
              <TableHead>Actions</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {settings.length === 0 ? (
              <TableRow>
                <TableCell
                  colSpan={3}
                  className="text-muted-foreground text-center"
                >
                  No settings yet.
                </TableCell>
              </TableRow>
            ) : (
              settings.map((setting) => (
                <TableRow key={setting.key}>
                  <TableCell className="font-mono text-[11px]">
                    {setting.key}
                  </TableCell>
                  <TableCell className="font-mono text-[11px]">
                    {setting.value}
                  </TableCell>
                  <TableCell className="space-x-2">
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => handleEditSetting(setting)}
                    >
                      Edit
                    </Button>
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => void handleDeleteSetting(setting.key)}
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

      <section className="space-y-2">
        <p className="text-sm font-semibold">Bindings</p>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Conversation</TableHead>
              <TableHead>Session</TableHead>
              <TableHead>Agent</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {bindings.length === 0 ? (
              <TableRow>
                <TableCell
                  colSpan={3}
                  className="text-muted-foreground text-center"
                >
                  No bindings.
                </TableCell>
              </TableRow>
            ) : (
              bindings.map((binding) => (
                <TableRow key={binding.conversation_key}>
                  <TableCell className="font-mono text-[11px]">
                    {binding.conversation_key}
                  </TableCell>
                  <TableCell className="font-mono text-[11px]">
                    {binding.session_id}
                  </TableCell>
                  <TableCell className="font-mono text-[11px]">
                    {binding.agent_id ?? "—"}
                  </TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </Table>
      </section>

      <section className="space-y-2">
        <p className="text-sm font-semibold">Active Sessions</p>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Session ID</TableHead>
              <TableHead>Users</TableHead>
              <TableHead>Updated</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {sessions.length === 0 ? (
              <TableRow>
                <TableCell
                  colSpan={3}
                  className="text-muted-foreground text-center"
                >
                  No active sessions.
                </TableCell>
              </TableRow>
            ) : (
              sessions.map((session) => (
                <TableRow key={session.session_id}>
                  <TableCell className="font-mono text-[11px]">
                    {session.session_id}
                  </TableCell>
                  <TableCell className="font-mono text-[11px]">
                    {session.users.join(", ")}
                  </TableCell>
                  <TableCell>
                    {new Date(session.updated_at).toLocaleString()}
                  </TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </Table>
      </section>
    </section>
  );
}
