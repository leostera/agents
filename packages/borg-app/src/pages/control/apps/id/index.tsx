import {
  type AppCapabilityRecord,
  type AppConnectionRecord,
  type AppRecord,
  createBorgApiClient,
} from "@borg/api";
import {
  Badge,
  Button,
  Input,
  Label,
  RadioGroup,
  RadioGroupItem,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
  Textarea,
} from "@borg/ui";
import { ChevronLeft, Pencil, Plus } from "lucide-react";
import React from "react";
import {
  AddCapabilityForm,
  type AddCapabilityInput,
} from "./AddCapabilityForm";

const borgApi = createBorgApiClient();

type AppCapabilitiesPageProps = {
  appId: string;
};

export function AppDetailsPage({ appId }: AppCapabilitiesPageProps) {
  const [app, setApp] = React.useState<AppRecord | null>(null);
  const [appDraft, setAppDraft] = React.useState({
    name: "",
    slug: "",
    description: "",
    status: "active",
    availableSecretsText: "",
  });
  const [capabilities, setCapabilities] = React.useState<AppCapabilityRecord[]>(
    []
  );
  const [connections, setConnections] = React.useState<AppConnectionRecord[]>(
    []
  );
  const [isLoading, setIsLoading] = React.useState(true);
  const [isSavingApp, setIsSavingApp] = React.useState(false);
  const [isSavingCapability, setIsSavingCapability] = React.useState(false);
  const [isChangingConnection, setIsChangingConnection] = React.useState(false);
  const [isAddCapabilityOpen, setIsAddCapabilityOpen] = React.useState(false);
  const [isEditCapabilityOpen, setIsEditCapabilityOpen] = React.useState(false);
  const [editingCapability, setEditingCapability] =
    React.useState<AppCapabilityRecord | null>(null);
  const [loadError, setLoadError] = React.useState<string | null>(null);
  const [actionError, setActionError] = React.useState<string | null>(null);

  const nextCapabilityId = React.useCallback((): string => {
    if (
      typeof crypto !== "undefined" &&
      typeof crypto.randomUUID === "function"
    ) {
      return `borg:capability:${crypto.randomUUID()}`;
    }
    return `borg:capability:${Date.now()}`;
  }, []);

  const load = React.useCallback(async () => {
    const normalizedAppId = appId.trim();
    if (!normalizedAppId) {
      setLoadError("Missing app id");
      setApp(null);
      setCapabilities([]);
      setConnections([]);
      setIsLoading(false);
      return;
    }

    setIsLoading(true);
    setLoadError(null);
    try {
      const [loadedApp, loadedCapabilities, loadedConnections] =
        await Promise.all([
          borgApi.getApp(normalizedAppId),
          borgApi.listAppCapabilities(normalizedAppId, 500),
          borgApi.listAppConnections(normalizedAppId, 500),
        ]);
      if (!loadedApp) {
        setLoadError("App not found");
        setApp(null);
        setCapabilities([]);
        setConnections([]);
        return;
      }

      setApp(loadedApp);
      setAppDraft({
        name: loadedApp.name,
        slug: loadedApp.slug,
        description: loadedApp.description,
        status: loadedApp.status,
        availableSecretsText: (loadedApp.available_secrets ?? []).join("\n"),
      });
      setCapabilities(loadedCapabilities);
      setConnections(loadedConnections);
    } catch (loadError) {
      setApp(null);
      setCapabilities([]);
      setConnections([]);
      setLoadError(
        loadError instanceof Error ? loadError.message : "Unable to load app"
      );
    } finally {
      setIsLoading(false);
    }
  }, [appId]);

  React.useEffect(() => {
    void load();
  }, [load]);

  const sortedCapabilities = React.useMemo(
    () =>
      [...capabilities].sort((a, b) =>
        a.name.localeCompare(b.name, undefined, { sensitivity: "base" })
      ),
    [capabilities]
  );

  const hasActiveConnection = React.useMemo(
    () =>
      connections.some(
        (connection) =>
          connection.status.trim().toLowerCase() === "connected" ||
          connection.status.trim().toLowerCase() === "pending_oauth"
      ),
    [connections]
  );

  const handleConnectOauth = async () => {
    if (!app) return;
    setActionError(null);
    setIsChangingConnection(true);
    try {
      const returnTo = `${window.location.origin}/control/apps/${encodeURIComponent(app.app_id)}`;
      const response = await borgApi.startAppOAuth(app.app_id, {
        return_to: returnTo,
      });
      window.location.assign(response.authorize_url);
    } catch (error) {
      setActionError(
        error instanceof Error ? error.message : "Unable to start OAuth sign-in"
      );
      setIsChangingConnection(false);
    }
  };

  const handleDisconnectOauth = async () => {
    if (!app) return;
    setActionError(null);
    setIsChangingConnection(true);
    try {
      for (const connection of connections) {
        const secrets = await borgApi.listAppSecrets(app.app_id, {
          limit: 500,
          connectionId: connection.connection_id,
        });
        await Promise.all(
          secrets.map((secret) =>
            borgApi.deleteAppSecret(app.app_id, secret.secret_id, {
              ignoreNotFound: true,
            })
          )
        );
        await borgApi.deleteAppConnection(
          app.app_id,
          connection.connection_id,
          {
            ignoreNotFound: true,
          }
        );
      }
      await load();
    } catch (error) {
      setActionError(
        error instanceof Error
          ? error.message
          : "Unable to disconnect app connection"
      );
    } finally {
      setIsChangingConnection(false);
    }
  };

  const handleAddCapability = async (input: AddCapabilityInput) => {
    const normalizedAppId = appId.trim();
    if (!normalizedAppId) {
      setActionError("Missing app id");
      return;
    }

    setActionError(null);
    setIsSavingCapability(true);
    try {
      const capabilityId = nextCapabilityId();
      await borgApi.upsertAppCapability(normalizedAppId, capabilityId, {
        name: input.name.trim(),
        hint: input.hint.trim(),
        mode: input.mode.trim() || "codemode",
        instructions: input.instructions.trim(),
        status: input.status.trim() || "active",
      });
      const createdCapability = await borgApi.getAppCapability(
        normalizedAppId,
        capabilityId
      );
      if (!createdCapability) {
        throw new Error("Capability could not be verified after save.");
      }
      setIsAddCapabilityOpen(false);
      await load();
    } catch (saveError) {
      setActionError(
        saveError instanceof Error
          ? saveError.message
          : "Unable to save capability"
      );
    } finally {
      setIsSavingCapability(false);
    }
  };

  const handleSaveApp = async () => {
    if (!app) return;
    setActionError(null);
    setIsSavingApp(true);
    try {
      const availableSecrets = appDraft.availableSecretsText
        .split("\n")
        .map((value) => value.trim())
        .filter((value) => value.length > 0);
      await borgApi.upsertApp(app.app_id, {
        name: appDraft.name.trim(),
        slug: appDraft.slug.trim(),
        description: appDraft.description.trim(),
        status: appDraft.status.trim() || "active",
        available_secrets: availableSecrets,
      });
      await load();
    } catch (saveError) {
      setActionError(
        saveError instanceof Error ? saveError.message : "Unable to save app"
      );
    } finally {
      setIsSavingApp(false);
    }
  };

  const handleEditCapability = async (input: AddCapabilityInput) => {
    const normalizedAppId = appId.trim();
    const capability = editingCapability;
    if (!normalizedAppId || !capability) {
      setActionError("Missing capability context");
      return;
    }

    setActionError(null);
    setIsSavingCapability(true);
    try {
      await borgApi.upsertAppCapability(
        normalizedAppId,
        capability.capability_id,
        {
          name: input.name.trim(),
          hint: input.hint.trim(),
          mode: input.mode.trim() || "codemode",
          instructions: input.instructions.trim(),
          status: input.status.trim() || "active",
        }
      );
      setIsEditCapabilityOpen(false);
      setEditingCapability(null);
      await load();
    } catch (saveError) {
      setActionError(
        saveError instanceof Error
          ? saveError.message
          : "Unable to save capability"
      );
    } finally {
      setIsSavingCapability(false);
    }
  };

  if (isLoading) {
    return <p className="text-muted-foreground text-sm">Loading app...</p>;
  }

  if (loadError) {
    return <p className="text-destructive text-sm">{loadError}</p>;
  }

  if (!app) {
    return <p className="text-muted-foreground text-sm">App not found.</p>;
  }

  return (
    <section className="space-y-4">
      {actionError ? (
        <p className="text-destructive text-sm">{actionError}</p>
      ) : null}
      <div>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => {
            window.history.pushState(null, "", "/control/apps");
            window.dispatchEvent(new PopStateEvent("popstate"));
          }}
        >
          <ChevronLeft className="size-4" />
          Back to Apps
        </Button>
      </div>

      <section className="grid gap-4 lg:grid-cols-2">
        <section className="space-y-3 rounded-md border p-4">
          <h2 className="text-sm font-semibold">App Details</h2>
          <div className="space-y-1">
            <p className="text-muted-foreground text-xs">ID</p>
            <Input value={app.app_id} readOnly disabled className="font-mono" />
          </div>
          <div className="space-y-1">
            <Label htmlFor="app-name">Name</Label>
            <Input
              id="app-name"
              value={appDraft.name}
              onChange={(event) =>
                setAppDraft((current) => ({
                  ...current,
                  name: event.currentTarget.value,
                }))
              }
            />
          </div>
          <div className="space-y-1">
            <Label htmlFor="app-slug">Slug</Label>
            <Input
              id="app-slug"
              value={appDraft.slug}
              onChange={(event) =>
                setAppDraft((current) => ({
                  ...current,
                  slug: event.currentTarget.value,
                }))
              }
            />
          </div>
          <div className="space-y-1">
            <Label>Status</Label>
            <div className="flex items-center gap-3 text-sm">
              <span>Status:</span>
              <RadioGroup
                value={appDraft.status}
                onValueChange={(value) =>
                  setAppDraft((current) => ({ ...current, status: value }))
                }
                className="flex items-center gap-4"
              >
                <label className="inline-flex items-center gap-2">
                  <RadioGroupItem value="active" id="app-status-active" />
                  <span>Active</span>
                </label>
                <label className="inline-flex items-center gap-2">
                  <RadioGroupItem value="disabled" id="app-status-disabled" />
                  <span>Disabled</span>
                </label>
              </RadioGroup>
            </div>
          </div>
          <div className="space-y-1">
            <Label htmlFor="app-description">Description</Label>
            <Textarea
              id="app-description"
              value={appDraft.description}
              onChange={(event) =>
                setAppDraft((current) => ({
                  ...current,
                  description: event.currentTarget.value,
                }))
              }
              rows={5}
            />
          </div>
          <div className="space-y-1">
            <Label htmlFor="app-available-secrets">Available secrets</Label>
            <Textarea
              id="app-available-secrets"
              value={appDraft.availableSecretsText}
              onChange={(event) =>
                setAppDraft((current) => ({
                  ...current,
                  availableSecretsText: event.currentTarget.value,
                }))
              }
              rows={4}
              placeholder={"APP_GITHUB_ACCESS_TOKEN\nAPP_GITHUB_REFRESH_TOKEN"}
            />
            <p className="text-muted-foreground text-xs">
              One secret name per line.
            </p>
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="space-y-1">
              <p className="text-muted-foreground text-xs">Created</p>
              <p className="text-xs">
                {new Date(app.created_at).toLocaleString()}
              </p>
            </div>
            <div className="space-y-1">
              <p className="text-muted-foreground text-xs">Updated</p>
              <p className="text-xs">
                {new Date(app.updated_at).toLocaleString()}
              </p>
            </div>
          </div>
          <div className="flex justify-end">
            <Button onClick={() => void handleSaveApp()} disabled={isSavingApp}>
              {isSavingApp ? "Saving..." : "Save"}
            </Button>
          </div>
          {app.auth_strategy?.trim().toLowerCase() === "oauth2" ? (
            <div className="rounded-md border p-3">
              <div className="mb-2 flex items-center justify-between gap-2">
                <p className="text-sm font-medium">Connection</p>
                <Badge variant={hasActiveConnection ? "default" : "outline"}>
                  {hasActiveConnection ? "Connected" : "Not connected"}
                </Badge>
              </div>
              <div className="flex items-center gap-2">
                <Button
                  type="button"
                  variant="outline"
                  disabled={isChangingConnection}
                  onClick={() => void handleConnectOauth()}
                >
                  {hasActiveConnection ? "Reconnect" : "Sign in with GitHub"}
                </Button>
                {hasActiveConnection ? (
                  <Button
                    type="button"
                    variant="destructive"
                    disabled={isChangingConnection}
                    onClick={() => void handleDisconnectOauth()}
                  >
                    Disconnect
                  </Button>
                ) : null}
              </div>
            </div>
          ) : null}
        </section>

        <section className="space-y-3 rounded-md border p-4">
          <div className="flex items-center justify-between gap-2">
            <h2 className="text-sm font-semibold">Capabilities</h2>
            <div className="flex items-center gap-2">
              <Badge variant="outline">{capabilities.length}</Badge>
              <Button
                size="icon-sm"
                variant="outline"
                onClick={() => setIsAddCapabilityOpen(true)}
                title="Add capability"
                aria-label="Add capability"
              >
                <Plus className="size-3.5" />
              </Button>
            </div>
          </div>

          <Table>
            <TableHeader>
              <TableRow>
                <TableHead className="w-[44px]">Status</TableHead>
                <TableHead>Name</TableHead>
                <TableHead>Mode</TableHead>
                <TableHead>Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {sortedCapabilities.length === 0 ? (
                <TableRow>
                  <TableCell
                    colSpan={4}
                    className="text-muted-foreground text-center"
                  >
                    No capabilities yet.
                  </TableCell>
                </TableRow>
              ) : (
                sortedCapabilities.map((capability) => (
                  <TableRow key={capability.capability_id}>
                    <TableCell>
                      <span
                        className={
                          capability.status.trim().toLowerCase() === "active"
                            ? "inline-block size-2.5 rounded-full bg-emerald-500"
                            : "inline-block size-2.5 rounded-full bg-rose-500"
                        }
                      />
                    </TableCell>
                    <TableCell>{capability.name}</TableCell>
                    <TableCell>{capability.mode}</TableCell>
                    <TableCell>
                      <Button
                        size="icon-sm"
                        variant="outline"
                        title="Edit capability"
                        aria-label={`Edit ${capability.name}`}
                        onClick={() => {
                          setEditingCapability(capability);
                          setIsEditCapabilityOpen(true);
                        }}
                      >
                        <Pencil className="size-3.5" />
                      </Button>
                    </TableCell>
                  </TableRow>
                ))
              )}
            </TableBody>
          </Table>
        </section>
      </section>

      <AddCapabilityForm
        open={isAddCapabilityOpen}
        onOpenChange={setIsAddCapabilityOpen}
        isSaving={isSavingCapability}
        onSubmit={handleAddCapability}
      />
      <AddCapabilityForm
        open={isEditCapabilityOpen}
        onOpenChange={(open) => {
          setIsEditCapabilityOpen(open);
          if (!open) setEditingCapability(null);
        }}
        isSaving={isSavingCapability}
        title="Edit Capability"
        description="Update capability settings for this app."
        submitLabel="Save Changes"
        initialValue={
          editingCapability
            ? {
                name: editingCapability.name,
                hint: editingCapability.hint,
                mode: editingCapability.mode,
                instructions: editingCapability.instructions,
                status: editingCapability.status,
              }
            : null
        }
        onSubmit={handleEditCapability}
      />
    </section>
  );
}
