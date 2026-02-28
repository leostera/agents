import {
  type AppCapabilityRecord,
  type AppRecord,
  createBorgApiClient,
} from "@borg/api";
import {
  Badge,
  Button,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@borg/ui";
import { ChevronLeft } from "lucide-react";
import React from "react";

const borgApi = createBorgApiClient();

type AppCapabilitiesPageProps = {
  appId: string;
};

export function AppDetailsPage({ appId }: AppCapabilitiesPageProps) {
  const [app, setApp] = React.useState<AppRecord | null>(null);
  const [capabilities, setCapabilities] = React.useState<AppCapabilityRecord[]>(
    []
  );
  const [isLoading, setIsLoading] = React.useState(true);
  const [error, setError] = React.useState<string | null>(null);

  const load = React.useCallback(async () => {
    const normalizedAppId = appId.trim();
    if (!normalizedAppId) {
      setError("Missing app id");
      setApp(null);
      setCapabilities([]);
      setIsLoading(false);
      return;
    }

    setIsLoading(true);
    setError(null);
    try {
      const [loadedApp, loadedCapabilities] = await Promise.all([
        borgApi.getApp(normalizedAppId),
        borgApi.listAppCapabilities(normalizedAppId, 500),
      ]);
      if (!loadedApp) {
        setError("App not found");
        setApp(null);
        setCapabilities([]);
        return;
      }

      setApp(loadedApp);
      setCapabilities(loadedCapabilities);
    } catch (loadError) {
      setApp(null);
      setCapabilities([]);
      setError(
        loadError instanceof Error ? loadError.message : "Unable to load app"
      );
    } finally {
      setIsLoading(false);
    }
  }, [appId]);

  React.useEffect(() => {
    void load();
  }, [load]);

  if (isLoading) {
    return <p className="text-muted-foreground text-sm">Loading app...</p>;
  }

  if (error) {
    return <p className="text-destructive text-sm">{error}</p>;
  }

  if (!app) {
    return <p className="text-muted-foreground text-sm">App not found.</p>;
  }

  return (
    <section className="space-y-4">
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
            <p className="font-mono text-[11px] break-all">{app.app_id}</p>
          </div>
          <div className="space-y-1">
            <p className="text-muted-foreground text-xs">Name</p>
            <p>{app.name}</p>
          </div>
          <div className="space-y-1">
            <p className="text-muted-foreground text-xs">Slug</p>
            <p className="font-mono text-[11px]">{app.slug}</p>
          </div>
          <div className="space-y-1">
            <p className="text-muted-foreground text-xs">Status</p>
            <Badge variant="outline">{app.status}</Badge>
          </div>
          <div className="space-y-1">
            <p className="text-muted-foreground text-xs">Description</p>
            <p className="text-sm whitespace-pre-wrap break-words">
              {app.description || "—"}
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
        </section>

        <section className="space-y-3 rounded-md border p-4">
          <div className="flex items-center justify-between gap-2">
            <h2 className="text-sm font-semibold">Capabilities</h2>
            <Badge variant="outline">{capabilities.length}</Badge>
          </div>

          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Name</TableHead>
                <TableHead>Mode</TableHead>
                <TableHead>Status</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {capabilities.length === 0 ? (
                <TableRow>
                  <TableCell
                    colSpan={3}
                    className="text-muted-foreground text-center"
                  >
                    No capabilities yet.
                  </TableCell>
                </TableRow>
              ) : (
                capabilities.map((capability) => (
                  <TableRow key={capability.capability_id}>
                    <TableCell>{capability.name}</TableCell>
                    <TableCell>{capability.mode}</TableCell>
                    <TableCell>{capability.status}</TableCell>
                  </TableRow>
                ))
              )}
            </TableBody>
          </Table>
        </section>
      </section>
    </section>
  );
}
