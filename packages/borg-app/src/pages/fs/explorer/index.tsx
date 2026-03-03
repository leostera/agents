import { createBorgApiClient, type FsFileRecord } from "@borg/api";
import {
  Badge,
  Input,
  Switch,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@borg/ui";
import { FolderTree } from "lucide-react";
import React from "react";
import {
  Section,
  SectionContent,
  SectionEmpty,
  SectionToolbar,
} from "../../../components/Section";

const borgApi = createBorgApiClient();

function bytesLabel(value: number): string {
  if (!Number.isFinite(value) || value < 0) return "—";
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  if (value < 1024 * 1024 * 1024)
    return `${(value / (1024 * 1024)).toFixed(1)} MB`;
  return `${(value / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

function fileKind(fileId: string): string {
  const parts = fileId.split(":");
  return parts.length >= 2 ? parts[1] : "unknown";
}

function formatTimestamp(value: string): string {
  const parsed = new Date(value);
  if (Number.isNaN(parsed.valueOf())) return "—";
  return parsed.toLocaleString();
}

export function FsExplorerPage() {
  const [files, setFiles] = React.useState<FsFileRecord[]>([]);
  const [isLoading, setIsLoading] = React.useState(true);
  const [error, setError] = React.useState<string | null>(null);
  const [query, setQuery] = React.useState(
    () => new URLSearchParams(window.location.search).get("q") ?? ""
  );
  const [includeDeleted, setIncludeDeleted] = React.useState(
    () => new URLSearchParams(window.location.search).get("deleted") === "1"
  );

  const loadFiles = React.useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const rows = await borgApi.listFsFiles({
        limit: 2000,
        q: query,
        includeDeleted,
      });
      setFiles(rows);
    } catch (loadError) {
      setFiles([]);
      setError(
        loadError instanceof Error ? loadError.message : "Unable to load files"
      );
    } finally {
      setIsLoading(false);
    }
  }, [includeDeleted, query]);

  React.useEffect(() => {
    void loadFiles();
  }, [loadFiles]);

  React.useEffect(() => {
    const params = new URLSearchParams();
    if (query.trim()) {
      params.set("q", query.trim());
    }
    if (includeDeleted) {
      params.set("deleted", "1");
    }
    const paramsText = params.toString();
    const next = paramsText ? `/fs/explorer?${paramsText}` : "/fs/explorer";
    window.history.replaceState(null, "", next);
  }, [includeDeleted, query]);

  return (
    <Section className="gap-4">
      <SectionToolbar className="justify-between">
        <Input
          value={query}
          onChange={(event) => setQuery(event.currentTarget.value)}
          placeholder="Search by file id, mime, storage key, or hash"
          aria-label="Search files"
        />
        <label className="flex items-center gap-2 text-sm whitespace-nowrap">
          <Switch
            checked={includeDeleted}
            onCheckedChange={(checked) => setIncludeDeleted(Boolean(checked))}
          />
          Include deleted
        </label>
      </SectionToolbar>

      {error ? <p className="text-destructive text-xs">{error}</p> : null}

      <SectionContent>
        {!isLoading && files.length === 0 ? (
          <SectionEmpty
            icon={FolderTree}
            title="No Files Found"
            description="No files match the current query yet."
          />
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Status</TableHead>
                <TableHead>File ID</TableHead>
                <TableHead>Kind</TableHead>
                <TableHead>MIME</TableHead>
                <TableHead>Size</TableHead>
                <TableHead>Created</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {isLoading ? (
                <TableRow>
                  <TableCell
                    colSpan={6}
                    className="text-muted-foreground text-center"
                  >
                    Loading files...
                  </TableCell>
                </TableRow>
              ) : (
                files.map((file) => (
                  <TableRow key={file.file_id}>
                    <TableCell>
                      {file.deleted_at ? (
                        <Badge variant="outline">Deleted</Badge>
                      ) : (
                        <Badge>Active</Badge>
                      )}
                    </TableCell>
                    <TableCell className="font-mono text-[11px]">
                      {file.file_id}
                    </TableCell>
                    <TableCell>{fileKind(file.file_id)}</TableCell>
                    <TableCell>{file.content_type || "—"}</TableCell>
                    <TableCell>{bytesLabel(file.size_bytes)}</TableCell>
                    <TableCell>{formatTimestamp(file.created_at)}</TableCell>
                  </TableRow>
                ))
              )}
            </TableBody>
          </Table>
        )}
      </SectionContent>
    </Section>
  );
}
