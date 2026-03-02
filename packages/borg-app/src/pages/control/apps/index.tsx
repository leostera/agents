import {
  type AppCapabilityRecord,
  type AppRecord,
  createBorgApiClient,
} from "@borg/api";
import {
  Badge,
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
  EntityLink,
  Input,
  Label,
  Switch,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
  Textarea,
} from "@borg/ui";
import {
  AppWindow,
  LoaderCircle,
  Pencil,
  Plus,
  Power,
  Trash2,
} from "lucide-react";
import React from "react";
import { AddAppForm, type AddAppInput } from "./AddAppForm";

const borgApi = createBorgApiClient();

type AppFormState = {
  appId: string;
  name: string;
  description: string;
  status: string;
};

const DEFAULT_FORM: AppFormState = {
  appId: "",
  name: "",
  description: "",
  status: "active",
};

function slugFromName(name: string): string {
  const slug = name
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return slug || "app";
}

function nextCapabilityId(): string {
  if (
    typeof crypto !== "undefined" &&
    typeof crypto.randomUUID === "function"
  ) {
    return `borg:capability:${crypto.randomUUID()}`;
  }
  return `borg:capability:${Date.now()}`;
}

function navigateToAppDetails(appId: string): void {
  window.history.pushState(
    null,
    "",
    `/control/apps/${encodeURIComponent(appId)}`
  );
  window.dispatchEvent(new PopStateEvent("popstate"));
}

export function AppsPage() {
  const [apps, setApps] = React.useState<AppRecord[]>([]);
  const [capabilitiesByAppId, setCapabilitiesByAppId] = React.useState<
    Record<string, AppCapabilityRecord[]>
  >({});
  const [query, setQuery] = React.useState(
    () => new URLSearchParams(window.location.search).get("q") ?? ""
  );
  const [isLoading, setIsLoading] = React.useState(true);
  const [error, setError] = React.useState<string | null>(null);
  const [isAddDialogOpen, setIsAddDialogOpen] = React.useState(false);
  const [isEditDialogOpen, setIsEditDialogOpen] = React.useState(false);
  const [isSaving, setIsSaving] = React.useState(false);
  const [statusUpdatingAppId, setStatusUpdatingAppId] = React.useState<
    string | null
  >(null);
  const [form, setForm] = React.useState<AppFormState>(DEFAULT_FORM);
  const [editingAppId, setEditingAppId] = React.useState<string | null>(null);

  const loadApps = React.useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const rows = await borgApi.listApps(500);
      setApps(rows);
      const capabilitiesRows = await Promise.all(
        rows.map(async (app) => {
          try {
            const capabilities = await borgApi.listAppCapabilities(
              app.app_id,
              500
            );
            return [app.app_id, capabilities] as const;
          } catch {
            return [app.app_id, []] as const;
          }
        })
      );
      setCapabilitiesByAppId(Object.fromEntries(capabilitiesRows));
    } catch (loadError) {
      setApps([]);
      setCapabilitiesByAppId({});
      setError(
        loadError instanceof Error ? loadError.message : "Unable to load apps"
      );
    } finally {
      setIsLoading(false);
    }
  }, []);

  React.useEffect(() => {
    void loadApps();
  }, [loadApps]);

  React.useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    if (query.trim()) {
      params.set("q", query.trim());
    } else {
      params.delete("q");
    }
    const paramsString = params.toString();
    const url = paramsString
      ? `/control/apps?${paramsString}`
      : "/control/apps";
    window.history.replaceState(null, "", url);
  }, [query]);

  const filteredApps = React.useMemo(() => {
    const term = query.trim().toLowerCase();
    if (!term) return apps;
    return apps.filter((app) =>
      [app.app_id, app.name, app.description, app.status]
        .join(" ")
        .toLowerCase()
        .includes(term)
    );
  }, [apps, query]);

  const openCreateDialog = () => {
    setIsAddDialogOpen(true);
  };

  const openEditDialog = (app: AppRecord) => {
    setEditingAppId(app.app_id);
    setForm({
      appId: app.app_id,
      name: app.name,
      description: app.description,
      status: app.status,
    });
    setIsEditDialogOpen(true);
  };

  const handleCreateSubmit = async (input: AddAppInput) => {
    setError(null);

    const appId = input.appId.trim();
    const name = input.name.trim();
    const slug = slugFromName(name);
    const description = input.description.trim();
    const status = input.status.trim() || "active";
    if (!appId || !name) {
      setError("App id and name are required.");
      return;
    }

    setIsSaving(true);
    try {
      const capabilityId = nextCapabilityId();
      await borgApi.upsertApp(appId, { name, slug, description, status });
      await borgApi.upsertAppCapability(appId, capabilityId, {
        name: input.capability.name.trim(),
        hint: input.capability.hint.trim(),
        mode: input.capability.mode.trim() || "codemode",
        instructions: input.capability.instructions.trim(),
        status: "active",
      });
      const createdCapability = await borgApi.getAppCapability(
        appId,
        capabilityId
      );
      if (!createdCapability) {
        throw new Error(
          "App was created, but initial capability could not be verified."
        );
      }
      setIsAddDialogOpen(false);
      await loadApps();
    } catch (saveError) {
      setError(
        saveError instanceof Error ? saveError.message : "Unable to save app"
      );
    } finally {
      setIsSaving(false);
    }
  };

  const handleStartGithubOAuth = async () => {
    setError(null);
    setIsSaving(true);
    try {
      const appId = "borg:app:github";
      const returnTo = `${window.location.origin}/control/apps/${encodeURIComponent(appId)}`;
      const response = await borgApi.startAppOAuth(appId, {
        return_to: returnTo,
      });
      window.location.assign(response.authorize_url);
    } catch (oauthError) {
      setError(
        oauthError instanceof Error
          ? oauthError.message
          : "Unable to start GitHub sign-in"
      );
    } finally {
      setIsSaving(false);
    }
  };

  const handleEditSubmit = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setError(null);

    const appId = form.appId.trim();
    const name = form.name.trim();
    const slug = slugFromName(name);
    const description = form.description.trim();
    const status = form.status.trim() || "active";
    if (!appId || !name) {
      setError("App id and name are required.");
      return;
    }

    setIsSaving(true);
    try {
      await borgApi.upsertApp(appId, { name, slug, description, status });
      setIsEditDialogOpen(false);
      setForm(DEFAULT_FORM);
      setEditingAppId(null);
      await loadApps();
    } catch (saveError) {
      setError(
        saveError instanceof Error ? saveError.message : "Unable to save app"
      );
    } finally {
      setIsSaving(false);
    }
  };

  const handleDeleteApp = async (app: AppRecord) => {
    const shouldDelete = window.confirm(
      `Delete app "${app.name}" (${app.app_id})?`
    );
    if (!shouldDelete) return;
    setError(null);
    try {
      await borgApi.deleteApp(app.app_id, { ignoreNotFound: true });
      await loadApps();
    } catch (deleteError) {
      setError(
        deleteError instanceof Error
          ? deleteError.message
          : "Unable to delete app"
      );
    }
  };

  const handleToggleAppStatus = async (app: AppRecord) => {
    const nextStatus =
      app.status.trim().toLowerCase() === "active" ? "disabled" : "active";
    setError(null);
    setStatusUpdatingAppId(app.app_id);
    try {
      await borgApi.upsertApp(app.app_id, {
        name: app.name,
        slug: app.slug,
        description: app.description,
        status: nextStatus,
      });
      await loadApps();
    } catch (toggleError) {
      setError(
        toggleError instanceof Error
          ? toggleError.message
          : "Unable to update app status"
      );
    } finally {
      setStatusUpdatingAppId(null);
    }
  };

  return (
    <section className="space-y-4">
      {isLoading || apps.length > 0 ? (
        <section className="flex flex-wrap items-center gap-2">
          <Input
            value={query}
            onChange={(event) => setQuery(event.currentTarget.value)}
            placeholder="Search apps"
            aria-label="Search apps"
            className="max-w-md"
          />
          <Button variant="outline" onClick={openCreateDialog}>
            <Plus className="size-4" />
            Add App
          </Button>
        </section>
      ) : null}

      {error ? <p className="text-destructive text-xs">{error}</p> : null}

      {!isLoading && filteredApps.length === 0 ? (
        <Empty className="border">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <AppWindow />
            </EmptyMedia>
            <EmptyTitle>No Apps Found</EmptyTitle>
            <EmptyDescription>
              Add your first app to expose capability discovery and execution.
            </EmptyDescription>
          </EmptyHeader>
          <EmptyContent className="flex-row justify-center">
            <Button onClick={openCreateDialog}>+ Add App</Button>
          </EmptyContent>
        </Empty>
      ) : (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead className="w-[44px]">Status</TableHead>
              <TableHead>Name</TableHead>
              <TableHead>Description</TableHead>
              <TableHead>Capabilities</TableHead>
              <TableHead>Updated</TableHead>
              <TableHead>Actions</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {isLoading ? (
              <TableRow>
                <TableCell
                  colSpan={6}
                  className="text-muted-foreground text-center"
                >
                  <span className="inline-flex items-center gap-2">
                    <LoaderCircle className="size-4 animate-spin" />
                    Loading apps...
                  </span>
                </TableCell>
              </TableRow>
            ) : (
              filteredApps.map((app) => (
                <TableRow
                  key={app.app_id}
                  className="cursor-pointer"
                  onClick={() => navigateToAppDetails(app.app_id)}
                >
                  <TableCell>
                    <span
                      className={
                        app.status.trim().toLowerCase() === "active"
                          ? "inline-block size-2.5 rounded-full bg-emerald-500"
                          : "inline-block size-2.5 rounded-full bg-rose-500"
                      }
                    />
                  </TableCell>
                  <TableCell>
                    <EntityLink uri={app.app_id} name={app.name} />
                  </TableCell>
                  <TableCell className="max-w-[280px]">
                    <p className="text-muted-foreground text-xs truncate">
                      {app.description || "—"}
                    </p>
                  </TableCell>
                  <TableCell>
                    <div className="flex flex-wrap gap-1">
                      {(capabilitiesByAppId[app.app_id] ?? [])
                        .slice(0, 3)
                        .map((capability) => (
                          <Badge
                            key={capability.capability_id}
                            variant="outline"
                          >
                            {capability.name}
                          </Badge>
                        ))}
                      {(capabilitiesByAppId[app.app_id] ?? []).length > 3 ? (
                        <Badge variant="outline">
                          +{(capabilitiesByAppId[app.app_id] ?? []).length - 3}
                        </Badge>
                      ) : null}
                      {(capabilitiesByAppId[app.app_id] ?? []).length === 0 ? (
                        <span className="text-muted-foreground text-xs">—</span>
                      ) : null}
                    </div>
                  </TableCell>
                  <TableCell>
                    {new Date(app.updated_at).toLocaleString()}
                  </TableCell>
                  <TableCell className="space-x-2">
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={(event) => {
                        event.stopPropagation();
                        void handleToggleAppStatus(app);
                      }}
                      disabled={statusUpdatingAppId === app.app_id}
                      title={
                        app.status.trim().toLowerCase() === "active"
                          ? "Disable app"
                          : "Enable app"
                      }
                      aria-label={
                        app.status.trim().toLowerCase() === "active"
                          ? `Disable ${app.name}`
                          : `Enable ${app.name}`
                      }
                    >
                      <Power className="size-3.5" />
                      {statusUpdatingAppId === app.app_id
                        ? "Updating..."
                        : app.status.trim().toLowerCase() === "active"
                          ? "Disable"
                          : "Enable"}
                    </Button>
                    <Button
                      size="icon-sm"
                      variant="outline"
                      onClick={(event) => {
                        event.stopPropagation();
                        openEditDialog(app);
                      }}
                      title="Edit app"
                      aria-label={`Edit ${app.name}`}
                    >
                      <Pencil className="size-3.5" />
                    </Button>
                    {!app.built_in ? (
                      <Button
                        size="icon-sm"
                        variant="outline"
                        onClick={(event) => {
                          event.stopPropagation();
                          void handleDeleteApp(app);
                        }}
                        title="Delete app"
                        aria-label={`Delete ${app.name}`}
                      >
                        <Trash2 className="size-3.5" />
                      </Button>
                    ) : null}
                  </TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </Table>
      )}

      <AddAppForm
        open={isAddDialogOpen}
        onOpenChange={setIsAddDialogOpen}
        isSaving={isSaving}
        onSubmit={handleCreateSubmit}
        onStartGithubOAuth={handleStartGithubOAuth}
      />

      <Dialog open={isEditDialogOpen} onOpenChange={setIsEditDialogOpen}>
        <DialogContent className="sm:max-w-xl">
          <DialogHeader>
            <DialogTitle>Edit App</DialogTitle>
            <DialogDescription>
              Configure app identity and lifecycle status.
            </DialogDescription>
          </DialogHeader>
          <form className="space-y-3" onSubmit={handleEditSubmit}>
            <div className="space-y-1">
              <Label htmlFor="app-id">App ID (URI)</Label>
              <Input
                id="app-id"
                value={form.appId}
                placeholder="borg:app:movieindex"
                required
                readOnly
                disabled
              />
            </div>
            <div className="space-y-1">
              <Label htmlFor="app-name">Name</Label>
              <Input
                id="app-name"
                value={form.name}
                onChange={(event) =>
                  setForm((current) => ({
                    ...current,
                    name: event.currentTarget.value,
                  }))
                }
                placeholder="MovieIndex"
                required
              />
            </div>
            <div className="space-y-1">
              <Label htmlFor="app-description">Description</Label>
              <Textarea
                id="app-description"
                value={form.description}
                onChange={(event) =>
                  setForm((current) => ({
                    ...current,
                    description: event.currentTarget.value,
                  }))
                }
                rows={4}
              />
            </div>
            <div className="space-y-1">
              <Label htmlFor="app-status">Status</Label>
              <div className="flex items-center gap-3">
                <Switch
                  id="app-status"
                  checked={form.status === "active"}
                  onCheckedChange={(checked) =>
                    setForm((current) => ({
                      ...current,
                      status: checked ? "active" : "disabled",
                    }))
                  }
                />
                <span className="text-sm">
                  {form.status === "active" ? "Active" : "Disabled"}
                </span>
              </div>
            </div>
            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                onClick={() => setIsEditDialogOpen(false)}
              >
                Cancel
              </Button>
              <Button type="submit" disabled={isSaving}>
                {isSaving ? "Saving..." : "Save App"}
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </section>
  );
}
