import { createBorgApiClient, type TaskGraphTask } from "@borg/api";
import {
  Badge,
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Input,
} from "@borg/ui";
import { LoaderCircle, Share2 } from "lucide-react";
import React from "react";

const borgApi = createBorgApiClient();

const STATUS_ORDER: Array<TaskGraphTask["status"]> = [
  "pending",
  "doing",
  "review",
  "done",
  "discarded",
];

function statusTone(status: TaskGraphTask["status"]): "secondary" | "default" {
  if (status === "review") return "default";
  return "secondary";
}

function navigateTo(href: string) {
  window.history.pushState(null, "", href);
  window.dispatchEvent(new PopStateEvent("popstate"));
}

export function TaskGraphExplorerPage() {
  const [isLoading, setIsLoading] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [query, setQuery] = React.useState("");
  const [tasks, setTasks] = React.useState<TaskGraphTask[]>([]);

  React.useEffect(() => {
    const load = async () => {
      setIsLoading(true);
      setError(null);
      try {
        const rows: TaskGraphTask[] = [];
        let cursor: string | null = null;
        for (;;) {
          const page = await borgApi.listTaskGraphTasks({ limit: 500, cursor });
          rows.push(...page.tasks);
          if (!page.nextCursor) break;
          cursor = page.nextCursor;
        }
        setTasks(rows);
      } catch (loadError) {
        setError(
          loadError instanceof Error
            ? loadError.message
            : "Unable to load tasks"
        );
      } finally {
        setIsLoading(false);
      }
    };

    void load();
  }, []);

  const filtered = React.useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return tasks;
    return tasks.filter((task) => {
      return (
        task.title.toLowerCase().includes(q) ||
        task.uri.toLowerCase().includes(q) ||
        task.labels.some((label) => label.toLowerCase().includes(q))
      );
    });
  }, [query, tasks]);

  const byStatus = React.useMemo(() => {
    const map = new Map<string, number>();
    for (const task of tasks) {
      map.set(task.status, (map.get(task.status) ?? 0) + 1);
    }
    return map;
  }, [tasks]);

  if (isLoading) {
    return (
      <p className="text-muted-foreground inline-flex items-center gap-2 text-xs">
        <LoaderCircle className="size-4 animate-spin" />
        Loading task graph...
      </p>
    );
  }

  return (
    <section className="space-y-4">
      <div className="flex flex-wrap items-center gap-2">
        <Input
          value={query}
          onChange={(event) => setQuery(event.currentTarget.value)}
          placeholder="Filter tasks by title, uri, or label"
          aria-label="Filter tasks"
          className="max-w-[36rem]"
        />
        {STATUS_ORDER.map((status) => (
          <Badge key={status} variant={statusTone(status)}>
            {status}: {byStatus.get(status) ?? 0}
          </Badge>
        ))}
      </div>

      {error ? <p className="text-destructive text-sm">{error}</p> : null}

      <section className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
        {filtered.map((task) => (
          <Card key={task.uri} className="border-dashed">
            <CardHeader className="pb-2">
              <CardTitle className="text-sm leading-tight">
                {task.title}
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-3 text-xs">
              <div className="flex flex-wrap items-center gap-2">
                <Badge variant={statusTone(task.status)}>{task.status}</Badge>
                {task.parent_uri ? (
                  <Badge variant="outline">child</Badge>
                ) : null}
              </div>

              <p className="text-muted-foreground truncate font-mono text-[11px]">
                {task.uri}
              </p>

              {task.blocked_by.length > 0 ? (
                <div className="space-y-1">
                  <p className="text-muted-foreground">Blocked by</p>
                  {task.blocked_by.slice(0, 3).map((uri) => (
                    <button
                      key={uri}
                      type="button"
                      className="text-primary block w-full truncate text-left font-mono text-[11px]"
                      onClick={() =>
                        navigateTo(
                          `/taskgraph/tasks/${encodeURIComponent(uri)}`
                        )
                      }
                    >
                      {uri}
                    </button>
                  ))}
                </div>
              ) : null}

              <div className="flex items-center justify-between gap-2">
                <span className="text-muted-foreground">
                  {task.labels.length} labels
                </span>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() =>
                    navigateTo(
                      `/taskgraph/tasks/${encodeURIComponent(task.uri)}`
                    )
                  }
                >
                  <Share2 className="size-4" />
                  Open
                </Button>
              </div>
            </CardContent>
          </Card>
        ))}
      </section>

      {filtered.length === 0 ? (
        <p className="text-muted-foreground text-sm">
          No tasks match this filter.
        </p>
      ) : null}
    </section>
  );
}
