import { createBorgApiClient, type TaskGraphTask } from "@borg/api";
import {
  Badge,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  ScrollArea,
} from "@borg/ui";
import { LoaderCircle } from "lucide-react";
import React from "react";

const borgApi = createBorgApiClient();

const STATUSES: Array<TaskGraphTask["status"]> = [
  "pending",
  "doing",
  "review",
  "done",
  "discarded",
];

function navigateTo(href: string) {
  window.history.pushState(null, "", href);
  window.dispatchEvent(new PopStateEvent("popstate"));
}

export function TaskGraphKanbanPage() {
  const [isLoading, setIsLoading] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
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

  const columns = React.useMemo(() => {
    const map = new Map<TaskGraphTask["status"], TaskGraphTask[]>();
    for (const status of STATUSES) {
      map.set(status, []);
    }
    for (const task of tasks) {
      map.get(task.status)?.push(task);
    }
    for (const status of STATUSES) {
      map.get(status)?.sort((a, b) => a.title.localeCompare(b.title));
    }
    return map;
  }, [tasks]);

  if (isLoading) {
    return (
      <p className="text-muted-foreground inline-flex items-center gap-2 text-xs">
        <LoaderCircle className="size-4 animate-spin" />
        Loading board...
      </p>
    );
  }

  if (error) {
    return <p className="text-destructive text-sm">{error}</p>;
  }

  return (
    <section className="grid min-h-0 gap-3 lg:grid-cols-5">
      {STATUSES.map((status) => {
        const items = columns.get(status) ?? [];
        return (
          <Card key={status} className="min-h-0">
            <CardHeader className="pb-2">
              <CardTitle className="text-sm capitalize">
                {status} <Badge variant="secondary">{items.length}</Badge>
              </CardTitle>
            </CardHeader>
            <CardContent className="min-h-0 p-2 pt-0">
              <ScrollArea className="h-[55vh] pr-2">
                <div className="space-y-2">
                  {items.map((task) => (
                    <button
                      key={task.uri}
                      type="button"
                      onClick={() =>
                        navigateTo(
                          `/taskgraph/tasks/${encodeURIComponent(task.uri)}`
                        )
                      }
                      className="bg-muted/40 hover:bg-muted block w-full rounded-md border p-2 text-left"
                    >
                      <p className="line-clamp-2 text-sm font-medium">
                        {task.title}
                      </p>
                      <p className="text-muted-foreground mt-1 truncate font-mono text-[10px]">
                        {task.uri}
                      </p>
                      <p className="text-muted-foreground mt-1 text-[11px]">
                        {task.blocked_by.length} deps · {task.labels.length}{" "}
                        labels
                      </p>
                    </button>
                  ))}
                  {items.length === 0 ? (
                    <p className="text-muted-foreground px-1 py-2 text-xs">
                      No tasks
                    </p>
                  ) : null}
                </div>
              </ScrollArea>
            </CardContent>
          </Card>
        );
      })}
    </section>
  );
}
