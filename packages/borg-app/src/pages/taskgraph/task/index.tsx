import {
  createBorgApiClient,
  type TaskGraphComment,
  type TaskGraphEvent,
  type TaskGraphTask,
} from "@borg/api";
import {
  Badge,
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@borg/ui";
import { ChevronLeft, LoaderCircle } from "lucide-react";
import React from "react";

const borgApi = createBorgApiClient();

type TaskGraphTaskDetailsPageProps = {
  taskUri: string;
};

function navigateTo(href: string) {
  window.history.pushState(null, "", href);
  window.dispatchEvent(new PopStateEvent("popstate"));
}

export function TaskGraphTaskDetailsPage({
  taskUri,
}: TaskGraphTaskDetailsPageProps) {
  const [isLoading, setIsLoading] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [task, setTask] = React.useState<TaskGraphTask | null>(null);
  const [children, setChildren] = React.useState<TaskGraphTask[]>([]);
  const [comments, setComments] = React.useState<TaskGraphComment[]>([]);
  const [events, setEvents] = React.useState<TaskGraphEvent[]>([]);

  React.useEffect(() => {
    const load = async () => {
      const target = taskUri.trim();
      if (!target) {
        setTask(null);
        setError("Missing task URI");
        return;
      }

      setIsLoading(true);
      setError(null);
      try {
        const [taskRow, childRows, commentRows, eventRows] = await Promise.all([
          borgApi.getTaskGraphTask(target),
          borgApi.listTaskGraphChildren(target, { limit: 200 }),
          borgApi.listTaskGraphComments(target, { limit: 200 }),
          borgApi.listTaskGraphEvents(target, { limit: 200 }),
        ]);

        if (!taskRow) {
          setTask(null);
          setError("Task not found");
          return;
        }

        setTask(taskRow);
        setChildren(childRows.children);
        setComments(commentRows.comments);
        setEvents(eventRows.events);
      } catch (loadError) {
        setTask(null);
        setError(
          loadError instanceof Error ? loadError.message : "Unable to load task"
        );
      } finally {
        setIsLoading(false);
      }
    };

    void load();
  }, [taskUri]);

  if (isLoading) {
    return (
      <p className="text-muted-foreground inline-flex items-center gap-2 text-xs">
        <LoaderCircle className="size-4 animate-spin" />
        Loading task details...
      </p>
    );
  }

  if (error) {
    return <p className="text-destructive text-sm">{error}</p>;
  }

  if (!task) {
    return <p className="text-muted-foreground text-sm">No task selected.</p>;
  }

  return (
    <section className="space-y-4">
      <div className="flex items-center gap-2">
        <Button
          variant="outline"
          size="sm"
          onClick={() => navigateTo("/taskgraph/kanban")}
        >
          <ChevronLeft className="size-4" />
          Back to board
        </Button>
        <Badge variant="secondary">{task.status}</Badge>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>{task.title}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3 text-sm">
          <p className="text-muted-foreground font-mono text-xs">{task.uri}</p>
          <p>{task.description || "No description."}</p>
          <p>
            <span className="font-medium">Definition of done:</span>{" "}
            {task.definition_of_done || "Not specified."}
          </p>
          <div className="flex flex-wrap gap-2 text-xs">
            <Badge variant="outline">assignee: {task.assignee_agent_id}</Badge>
            <Badge variant="outline">reviewer: {task.reviewer_agent_id}</Badge>
            <Badge variant="outline">labels: {task.labels.length}</Badge>
            <Badge variant="outline">deps: {task.blocked_by.length}</Badge>
          </div>
        </CardContent>
      </Card>

      <section className="grid gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle className="text-base">
              Children ({children.length})
            </CardTitle>
          </CardHeader>
          <CardContent>
            {children.length === 0 ? (
              <p className="text-muted-foreground text-sm">No child tasks.</p>
            ) : (
              <div className="space-y-2">
                {children.map((child) => (
                  <button
                    key={child.uri}
                    type="button"
                    className="hover:bg-muted w-full rounded border p-2 text-left"
                    onClick={() =>
                      navigateTo(
                        `/taskgraph/tasks/${encodeURIComponent(child.uri)}`
                      )
                    }
                  >
                    <p className="text-sm font-medium">{child.title}</p>
                    <p className="text-muted-foreground truncate font-mono text-xs">
                      {child.uri}
                    </p>
                  </button>
                ))}
              </div>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-base">
              Comments ({comments.length})
            </CardTitle>
          </CardHeader>
          <CardContent>
            {comments.length === 0 ? (
              <p className="text-muted-foreground text-sm">No comments yet.</p>
            ) : (
              <div className="space-y-2">
                {comments.map((comment) => (
                  <div key={comment.id} className="rounded border p-2 text-xs">
                    <p className="text-muted-foreground font-mono">
                      {comment.author_session_uri}
                    </p>
                    <p className="mt-1 whitespace-pre-wrap text-sm">
                      {comment.body}
                    </p>
                  </div>
                ))}
              </div>
            )}
          </CardContent>
        </Card>
      </section>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">Events ({events.length})</CardTitle>
        </CardHeader>
        <CardContent>
          {events.length === 0 ? (
            <p className="text-muted-foreground text-sm">
              No audit events recorded.
            </p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>When</TableHead>
                  <TableHead>Type</TableHead>
                  <TableHead>Actor Session</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {events.map((event) => (
                  <TableRow key={event.id}>
                    <TableCell className="text-xs">
                      {event.created_at}
                    </TableCell>
                    <TableCell>{event.type}</TableCell>
                    <TableCell className="font-mono text-xs">
                      {event.actor_session_uri}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>
    </section>
  );
}
