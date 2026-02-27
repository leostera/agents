import { createBorgApiClient } from "@borg/api";
import {
  Badge,
  Button,
  EntityView,
  Input,
  Link,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@borg/ui";
import { ExternalLink, LoaderCircle, Workflow } from "lucide-react";
import React from "react";

type MemoryEntity = {
  entity_id: string;
  entity_type: string;
  label: string;
  props?: Record<string, unknown>;
};

const borgApi = createBorgApiClient();
const URI_PATTERN = /^[a-z][a-z0-9+.-]*:[^:\s]+:[^:\s]+$/i;

type MemoryExplorerPageProps = {
  explorerUri?: string;
};

export function MemoryExplorerPage({ explorerUri }: MemoryExplorerPageProps) {
  const [query, setQuery] = React.useState(() => {
    if (explorerUri?.trim()) return explorerUri;
    if (typeof window === "undefined") return "";
    return new URLSearchParams(window.location.search).get("q") ?? "";
  });
  const [isLoading, setIsLoading] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [results, setResults] = React.useState<MemoryEntity[]>([]);
  const [singleResult, setSingleResult] = React.useState<MemoryEntity | null>(
    null
  );

  const runSearch = React.useCallback(
    async (nextQuery: string, syncUrl = true) => {
      const trimmedQuery = nextQuery.trim();
      const isUriQuery = URI_PATTERN.test(trimmedQuery);

      if (syncUrl && typeof window !== "undefined") {
        let nextUrl = "/memory/explorer";
        if (trimmedQuery) {
          if (isUriQuery) {
            nextUrl = `/memory/explorer/${trimmedQuery}`;
          } else {
            nextUrl = `/memory/explorer?q=${encodeURIComponent(trimmedQuery)}`;
          }
        }
        window.history.replaceState(null, "", nextUrl);
      }

      if (!trimmedQuery) {
        setResults([]);
        setSingleResult(null);
        setError(null);
        return;
      }

      setIsLoading(true);
      setError(null);
      try {
        if (isUriQuery) {
          const entity = await borgApi.getMemoryEntity(trimmedQuery);
          if (entity) {
            setSingleResult(entity as MemoryEntity);
            setResults([]);
            return;
          }
        }

        const rows = (await borgApi.searchMemory({
          q: trimmedQuery,
          limit: 100,
        })) as MemoryEntity[];
        setResults(rows);
        setSingleResult(rows.length === 1 ? rows[0] : null);
      } catch (searchError) {
        setResults([]);
        setSingleResult(null);
        setError(
          searchError instanceof Error
            ? searchError.message
            : "Unable to search memory"
        );
      } finally {
        setIsLoading(false);
      }
    },
    []
  );

  React.useEffect(() => {
    const initialQuery = explorerUri?.trim() ?? query.trim();
    if (initialQuery) {
      setQuery(initialQuery);
      void runSearch(initialQuery, false);
      return;
    }
    setResults([]);
    setSingleResult(null);
    setError(null);
  }, [explorerUri, runSearch]);

  const hasManyResults = results.length > 1;
  const singleEntity =
    singleResult ?? (results.length === 1 ? results[0] : null);
  const selectedUri = singleEntity?.entity_id ?? "";
  const graphHref = selectedUri ? `/memory/graph?query=${selectedUri}` : "";
  const entityHref = selectedUri ? `/memory/entity/${selectedUri}` : "";
  const navigateTo = React.useCallback((href: string) => {
    if (!href) return;
    window.history.pushState(null, "", href);
    window.dispatchEvent(new PopStateEvent("popstate"));
  }, []);

  return (
    <section className="space-y-4">
      <form
        className="flex flex-wrap items-center justify-between gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          void runSearch(query);
        }}
      >
        <div className="flex w-full max-w-[40rem] items-center gap-2">
          <Input
            autoFocus
            value={query}
            onChange={(event) => setQuery(event.currentTarget.value)}
            placeholder="Search entities or paste a URI (borg:user:leostera)"
            aria-label="Search memory"
          />
          <Button type="submit" variant="outline" disabled={isLoading}>
            {isLoading ? (
              <>
                <LoaderCircle className="size-4 animate-spin" />
                Searching
              </>
            ) : (
              "Search"
            )}
          </Button>
        </div>
        <div className="flex items-center justify-end gap-2">
          {singleEntity ? (
            <>
              <Button asChild variant="outline" size="sm">
                <Link href={graphHref}>
                  <Workflow className="size-4" />
                  Open Graph
                </Link>
              </Button>
              <Button asChild variant="outline" size="sm">
                <Link href={entityHref}>
                  <ExternalLink className="size-4" />
                  Open Entity
                </Link>
              </Button>
            </>
          ) : null}
        </div>
      </form>

      {error ? <p className="text-destructive text-xs">{error}</p> : null}

      {hasManyResults ? (
        <section className="space-y-2">
          <p className="text-muted-foreground text-xs">
            Results <Badge variant="secondary">{results.length}</Badge>
          </p>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Label</TableHead>
                <TableHead>URI</TableHead>
                <TableHead>Kind</TableHead>
                <TableHead className="text-right">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {results.map((entity) => (
                <TableRow
                  key={entity.entity_id}
                  className="cursor-pointer"
                  onClick={() =>
                    navigateTo(`/memory/entity/${entity.entity_id}`)
                  }
                >
                  <TableCell className="max-w-56 truncate font-medium">
                    {entity.label || "—"}
                  </TableCell>
                  <TableCell className="max-w-80 truncate font-mono text-[11px]">
                    {entity.entity_id}
                  </TableCell>
                  <TableCell>{entity.entity_type || "unknown"}</TableCell>
                  <TableCell className="text-right">
                    <div className="inline-flex gap-1">
                      <Button
                        asChild
                        variant="ghost"
                        size="sm"
                        onClick={(event) => {
                          event.stopPropagation();
                        }}
                      >
                        <Link href={`/memory/entity/${entity.entity_id}`}>
                          Open
                        </Link>
                      </Button>
                      <Button
                        asChild
                        variant="ghost"
                        size="sm"
                        onClick={(event) => {
                          event.stopPropagation();
                        }}
                      >
                        <Link href={`/memory/graph?query=${entity.entity_id}`}>
                          Graph
                        </Link>
                      </Button>
                    </div>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </section>
      ) : (
        <section>
          {singleEntity ? (
            <EntityView
              uri={singleEntity.entity_id}
              kind={singleEntity.entity_type}
              fields={singleEntity.props}
            />
          ) : (
            <p className="text-muted-foreground text-sm">
              {query.trim().length === 0
                ? "Search for an entity URI or keyword."
                : "No entities found."}
            </p>
          )}
        </section>
      )}
    </section>
  );
}
