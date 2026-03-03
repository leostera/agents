import { createBorgApiClient, type TaskGraphTask } from "@borg/api";
import {
  Badge,
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Input,
  ScrollArea,
} from "@borg/ui";
import { LoaderCircle, Plus, Save } from "lucide-react";
import React from "react";

const borgApi = createBorgApiClient();

const STATUSES: Array<TaskGraphTask["status"]> = [
  "pending",
  "doing",
  "review",
  "done",
  "discarded",
];

const STATUS_LABEL: Record<TaskGraphTask["status"], string> = {
  pending: "Backlog",
  doing: "Doing",
  review: "Review",
  done: "Done",
  discarded: "Archived",
};

const BOARD_SESSION_STORAGE_KEY = "taskgraph.board.session_uri";
const BOARD_ACTOR_STORAGE_KEY = "taskgraph.board.actor_id";

function navigateTo(href: string) {
  window.history.pushState(null, "", href);
  window.dispatchEvent(new PopStateEvent("popstate"));
}

function readStorage(key: string, fallback: string): string {
  if (typeof window === "undefined") return fallback;
  return window.localStorage.getItem(key)?.trim() || fallback;
}

function writeStorage(key: string, value: string) {
  if (typeof window === "undefined") return;
  window.localStorage.setItem(key, value);
}

export function TaskGraphKanbanPage() {
  const [isLoading, setIsLoading] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [tasks, setTasks] = React.useState<TaskGraphTask[]>([]);

  const [boardSessionUri, setBoardSessionUri] = React.useState(() =>
    readStorage(BOARD_SESSION_STORAGE_KEY, "borg:session:taskgraph-ui")
  );
  const [boardActorId, setBoardActorId] = React.useState(() =>
    readStorage(BOARD_ACTOR_STORAGE_KEY, "borg:actor:taskgraph-ui")
  );

  const [newTitle, setNewTitle] = React.useState("");
  const [newDescription, setNewDescription] = React.useState("");
  const [isCreating, setIsCreating] = React.useState(false);
  const [movingByUri, setMovingByUri] = React.useState<Record<string, boolean>>(
    {}
  );
  const [editingTitleByUri, setEditingTitleByUri] = React.useState<
    Record<string, string>
  >({});
  const [savingTitleByUri, setSavingTitleByUri] = React.useState<
    Record<string, boolean>
  >({});

  const load = React.useCallback(async () => {
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
      setEditingTitleByUri(
        Object.fromEntries(rows.map((task) => [task.uri, task.title]))
      );
    } catch (loadError) {
      setError(
        loadError instanceof Error ? loadError.message : "Unable to load tasks"
      );
    } finally {
      setIsLoading(false);
    }
  }, []);

  React.useEffect(() => {
    void load();
  }, [load]);

  React.useEffect(() => {
    writeStorage(BOARD_SESSION_STORAGE_KEY, boardSessionUri);
  }, [boardSessionUri]);

  React.useEffect(() => {
    writeStorage(BOARD_ACTOR_STORAGE_KEY, boardActorId);
  }, [boardActorId]);

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

  const createCard = async (event: React.FormEvent) => {
    event.preventDefault();
    const title = newTitle.trim();
    if (!title) return;

    setIsCreating(true);
    setError(null);
    try {
      await borgApi.createTaskGraphTask({
        sessionUri: boardSessionUri.trim(),
        creatorActorId: boardActorId.trim(),
        title,
        description: newDescription.trim(),
        definitionOfDone: "",
        assigneeActorId: boardActorId.trim(),
        labels: [],
      });
      setNewTitle("");
      setNewDescription("");
      await load();
    } catch (createError) {
      setError(
        createError instanceof Error
          ? createError.message
          : "Unable to create card"
      );
    } finally {
      setIsCreating(false);
    }
  };

  const moveCard = async (
    task: TaskGraphTask,
    status: TaskGraphTask["status"]
  ) => {
    const actorSessionUri =
      status === "discarded"
        ? task.reviewer_session_uri
        : task.assignee_session_uri;
    setMovingByUri((value) => ({ ...value, [task.uri]: true }));
    setError(null);
    try {
      await borgApi.setTaskGraphTaskStatus(task.uri, {
        sessionUri: actorSessionUri,
        status,
      });
      await load();
    } catch (moveError) {
      setError(
        moveError instanceof Error ? moveError.message : "Unable to move card"
      );
    } finally {
      setMovingByUri((value) => ({ ...value, [task.uri]: false }));
    }
  };

  const saveCardTitle = async (task: TaskGraphTask) => {
    const nextTitle = (editingTitleByUri[task.uri] ?? task.title).trim();
    if (!nextTitle || nextTitle === task.title) return;

    setSavingTitleByUri((value) => ({ ...value, [task.uri]: true }));
    setError(null);
    try {
      await borgApi.updateTaskGraphTaskFields(task.uri, {
        sessionUri: task.assignee_session_uri,
        title: nextTitle,
      });
      await load();
    } catch (saveError) {
      setError(
        saveError instanceof Error
          ? saveError.message
          : "Unable to save card title"
      );
    } finally {
      setSavingTitleByUri((value) => ({ ...value, [task.uri]: false }));
    }
  };

  if (isLoading) {
    return (
      <p className="text-muted-foreground inline-flex items-center gap-2 text-xs">
        <LoaderCircle className="size-4 animate-spin" />
        Loading board...
      </p>
    );
  }

  return (
    <section className="space-y-4">
      <Card>
        <CardContent className="grid gap-2 p-3 md:grid-cols-2">
          <div>
            <p className="text-muted-foreground mb-1 text-xs">
              Board Session URI
            </p>
            <Input
              value={boardSessionUri}
              onChange={(event) =>
                setBoardSessionUri(event.currentTarget.value)
              }
            />
          </div>
          <div>
            <p className="text-muted-foreground mb-1 text-xs">Board Actor ID</p>
            <Input
              value={boardActorId}
              onChange={(event) => setBoardActorId(event.currentTarget.value)}
            />
          </div>
        </CardContent>
      </Card>

      {error ? <p className="text-destructive text-sm">{error}</p> : null}

      <ScrollArea className="w-full whitespace-nowrap pb-2">
        <section className="grid min-h-0 grid-flow-col auto-cols-[20rem] gap-3">
          {STATUSES.map((status) => {
            const items = columns.get(status) ?? [];
            return (
              <Card key={status} className="min-h-0">
                <CardHeader className="pb-2">
                  <CardTitle className="text-sm">
                    {STATUS_LABEL[status]}{" "}
                    <Badge variant="secondary">{items.length}</Badge>
                  </CardTitle>
                </CardHeader>
                <CardContent className="min-h-0 space-y-2 p-2 pt-0">
                  {status === "pending" ? (
                    <form
                      className="space-y-2 rounded-md border p-2"
                      onSubmit={createCard}
                    >
                      <Input
                        placeholder="Add a card title"
                        value={newTitle}
                        onChange={(event) =>
                          setNewTitle(event.currentTarget.value)
                        }
                      />
                      <Input
                        placeholder="Description (optional)"
                        value={newDescription}
                        onChange={(event) =>
                          setNewDescription(event.currentTarget.value)
                        }
                      />
                      <Button type="submit" size="sm" disabled={isCreating}>
                        {isCreating ? (
                          <LoaderCircle className="size-4 animate-spin" />
                        ) : (
                          <Plus className="size-4" />
                        )}
                        Add card
                      </Button>
                    </form>
                  ) : null}

                  <ScrollArea className="h-[58vh] pr-2">
                    <div className="space-y-2">
                      {items.map((task) => {
                        const isMoving = movingByUri[task.uri] ?? false;
                        const isSavingTitle =
                          savingTitleByUri[task.uri] ?? false;
                        const titleDraft =
                          editingTitleByUri[task.uri] ?? task.title;

                        return (
                          <div
                            key={task.uri}
                            className="bg-muted/40 rounded-md border p-2"
                          >
                            <div className="mb-2 flex items-start gap-1">
                              <Input
                                className="h-8 text-sm"
                                value={titleDraft}
                                onChange={(event) =>
                                  setEditingTitleByUri((value) => ({
                                    ...value,
                                    [task.uri]: event.currentTarget.value,
                                  }))
                                }
                              />
                              <Button
                                size="icon"
                                variant="outline"
                                disabled={isSavingTitle}
                                onClick={() => void saveCardTitle(task)}
                              >
                                {isSavingTitle ? (
                                  <LoaderCircle className="size-4 animate-spin" />
                                ) : (
                                  <Save className="size-4" />
                                )}
                              </Button>
                            </div>

                            <button
                              type="button"
                              className="text-muted-foreground block w-full truncate text-left font-mono text-[10px]"
                              onClick={() =>
                                navigateTo(
                                  `/taskgraph/tasks/${encodeURIComponent(task.uri)}`
                                )
                              }
                            >
                              {task.uri}
                            </button>

                            <div className="mt-2 flex flex-wrap items-center gap-1">
                              {STATUSES.filter(
                                (candidate) => candidate !== task.status
                              )
                                .slice(0, 3)
                                .map((candidate) => (
                                  <Button
                                    key={candidate}
                                    size="sm"
                                    variant="ghost"
                                    className="h-6 px-2 text-[11px]"
                                    disabled={isMoving}
                                    onClick={() =>
                                      void moveCard(task, candidate)
                                    }
                                  >
                                    {candidate}
                                  </Button>
                                ))}
                            </div>
                          </div>
                        );
                      })}

                      {items.length === 0 ? (
                        <p className="text-muted-foreground px-1 py-2 text-xs">
                          No cards
                        </p>
                      ) : null}
                    </div>
                  </ScrollArea>
                </CardContent>
              </Card>
            );
          })}
        </section>
      </ScrollArea>
    </section>
  );
}
