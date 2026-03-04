import { createBorgApiClient } from "@borg/api";
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
import { Settings2 } from "lucide-react";
import React from "react";
import {
  Section,
  SectionContent,
  SectionEmpty,
  SectionToolbar,
} from "../../../components/Section";

const borgApi = createBorgApiClient();

type FsSettingsState = {
  backend: string;
  rootPath: string | null;
  counts: {
    total: number;
    active: number;
    deleted: number;
  };
};

export function FsSettingsPage() {
  const [settings, setSettings] = React.useState<FsSettingsState | null>(null);
  const [isLoading, setIsLoading] = React.useState(true);
  const [error, setError] = React.useState<string | null>(null);

  const loadSettings = React.useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const current = await borgApi.getFsSettings();
      setSettings(current);
    } catch (loadError) {
      setSettings(null);
      setError(
        loadError instanceof Error
          ? loadError.message
          : "Unable to load FS settings"
      );
    } finally {
      setIsLoading(false);
    }
  }, []);

  React.useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

  return (
    <Section className="gap-4">
      <SectionToolbar className="justify-end">
        <Button variant="outline" onClick={() => void loadSettings()}>
          Refresh
        </Button>
      </SectionToolbar>

      {error ? <p className="text-destructive text-xs">{error}</p> : null}

      <SectionContent>
        {!isLoading && !settings ? (
          <SectionEmpty
            icon={Settings2}
            title="No FS Settings Available"
            description="FS settings could not be loaded from the API."
          />
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Key</TableHead>
                <TableHead>Value</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {isLoading || !settings ? (
                <TableRow>
                  <TableCell
                    colSpan={2}
                    className="text-muted-foreground text-center"
                  >
                    Loading FS settings...
                  </TableCell>
                </TableRow>
              ) : (
                <>
                  <TableRow>
                    <TableCell>Backend</TableCell>
                    <TableCell>
                      <Badge variant="outline">{settings.backend}</Badge>
                    </TableCell>
                  </TableRow>
                  <TableRow>
                    <TableCell>Root Path</TableCell>
                    <TableCell className="font-mono text-[11px]">
                      {settings.rootPath ?? "—"}
                    </TableCell>
                  </TableRow>
                  <TableRow>
                    <TableCell>Total Files</TableCell>
                    <TableCell>{settings.counts.total}</TableCell>
                  </TableRow>
                  <TableRow>
                    <TableCell>Active Files</TableCell>
                    <TableCell>{settings.counts.active}</TableCell>
                  </TableRow>
                  <TableRow>
                    <TableCell>Deleted Files</TableCell>
                    <TableCell>{settings.counts.deleted}</TableCell>
                  </TableRow>
                </>
              )}
            </TableBody>
          </Table>
        )}
      </SectionContent>
    </Section>
  );
}
