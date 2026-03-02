import {
  type ActorRecord,
  type ClockworkJobRecord,
  createBorgApiClient,
} from "@borg/api";
import {
  Badge,
  Button,
  Input,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@borg/ui";
import { Clock3, LoaderCircle, Pause, Pencil, Play, Plus, Trash2 } from "lucide-react";
import React from "react";
import {
  Section,
  SectionContent,
  SectionEmpty,
  SectionToolbar,
} from "../../../components/Section";
import { AddClockworkJobForm, type ClockworkJobFormInput } from "./AddClockworkJobForm";

const borgApi = createBorgApiClient();

type DialogState =
  | { mode: "create" }
  | {
      mode: "edit";
      job: ClockworkJobRecord;
    };

function createClockworkJobId(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return `borg:clockwork_job:${crypto.randomUUID()}`;
  }
  return `borg:clockwork_job:${Date.now()}-${Math.floor(Math.random() * 1_000_000)}`;
}

function parseScheduleInput(form: ClockworkJobFormInput): {
  scheduleSpec: Record<string, unknown>;
  nextRunAt: string | null;
} {
  if (form.kind === "once") {
    const runDate = new Date(form.runAt);
    return {
      scheduleSpec: {
        kind: "once",
        run_at: runDate.toISOString(),
      },
      nextRunAt: runDate.toISOString(),
    };
  }

  return {
    scheduleSpec: {
      kind: "cron",
      cron: form.cronExpression.trim(),
    },
    nextRunAt: null,
  };
}

function toFormInput(job: ClockworkJobRecord): ClockworkJobFormInput {
  const schedule = job.schedule_spec ?? {};
  const scheduleKind = schedule.kind;
  const kind = scheduleKind === "cron" ? "cron" : "once";
  return {
    kind,
    actorId: job.target_actor_id,
    sessionId: job.target_session_id,
    messageText:
      typeof job.payload === "object" &&
      job.payload !== null &&
      "text" in job.payload &&
      typeof (job.payload as { text?: unknown }).text === "string"
        ? String((job.payload as { text: string }).text)
        : JSON.stringify(job.payload),
    runAt:
      typeof schedule.run_at === "string"
        ? schedule.run_at
        : typeof job.next_run_at === "string"
          ? job.next_run_at
          : "",
    cronExpression:
      typeof schedule.cron === "string" ? schedule.cron : "*/5 * * * *",
  };
}

export function ClockworkPage() {
  const [jobs, setJobs] = React.useState<ClockworkJobRecord[]>([]);
  const [actors, setActors] = React.useState<ActorRecord[]>([]);
  const [isLoading, setIsLoading] = React.useState(true);
  const [isSaving, setIsSaving] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [query, setQuery] = React.useState("");
  const [dialogState, setDialogState] = React.useState<DialogState | null>(null);

  const load = React.useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const [jobRows, actorRows] = await Promise.all([
        borgApi.listClockworkJobs({ limit: 500 }),
        borgApi.listActors(500),
      ]);
      setJobs(jobRows);
      setActors(actorRows);
    } catch (loadError) {
      setJobs([]);
      setError(
        loadError instanceof Error ? loadError.message : "Unable to load Clockwork"
      );
    } finally {
      setIsLoading(false);
    }
  }, []);

  React.useEffect(() => {
    void load();
  }, [load]);

  const filteredJobs = React.useMemo(() => {
    const term = query.trim().toLowerCase();
    if (!term) return jobs;
    return jobs.filter((job) =>
      [
        job.job_id,
        job.kind,
        job.status,
        actors.find((actor) => actor.actor_id === job.target_actor_id)?.name ?? "",
        job.target_actor_id,
        job.target_session_id,
        job.message_type,
      ]
        .join(" ")
        .toLowerCase()
        .includes(term)
    );
  }, [actors, jobs, query]);

  const actorNameById = React.useMemo(() => {
    const map = new Map<string, string>();
    for (const actor of actors) {
      map.set(actor.actor_id, actor.name);
    }
    return map;
  }, [actors]);

  const hasNoJobs = !isLoading && jobs.length === 0;

  const handleSubmit = async (input: ClockworkJobFormInput) => {
    const { scheduleSpec, nextRunAt } = parseScheduleInput(input);

    setError(null);
    setIsSaving(true);
    try {
      if (dialogState?.mode === "edit") {
        await borgApi.updateClockworkJob(dialogState.job.job_id, {
          kind: input.kind,
          actorId: input.actorId,
          sessionId: input.sessionId,
          messagePayload: { text: input.messageText },
          messageHeaders: {},
          scheduleSpec,
          nextRunAt,
        });
      } else {
        await borgApi.createClockworkJob({
          jobId: createClockworkJobId(),
          kind: input.kind,
          actorId: input.actorId,
          sessionId: input.sessionId,
          messagePayload: { text: input.messageText },
          messageHeaders: {},
          scheduleSpec,
          nextRunAt,
        });
      }
      setDialogState(null);
      await load();
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : "Unable to save job");
    } finally {
      setIsSaving(false);
    }
  };

  const handlePause = async (jobId: string) => {
    setError(null);
    try {
      await borgApi.pauseClockworkJob(jobId);
      await load();
    } catch (actionError) {
      setError(actionError instanceof Error ? actionError.message : "Unable to pause job");
    }
  };

  const handleResume = async (jobId: string) => {
    setError(null);
    try {
      await borgApi.resumeClockworkJob(jobId);
      await load();
    } catch (actionError) {
      setError(actionError instanceof Error ? actionError.message : "Unable to resume job");
    }
  };

  const handleCancel = async (jobId: string) => {
    setError(null);
    try {
      await borgApi.cancelClockworkJob(jobId);
      await load();
    } catch (actionError) {
      setError(actionError instanceof Error ? actionError.message : "Unable to cancel job");
    }
  };

  const formInitialValue =
    dialogState?.mode === "edit"
      ? toFormInput(dialogState.job)
      : {
          kind: "once" as const,
          actorId: actors[0]?.actor_id ?? "",
          sessionId: "",
          messageText: "",
          runAt: new Date().toISOString(),
          cronExpression: "*/5 * * * *",
        };

  return (
    <Section className="gap-4">
      {hasNoJobs ? null : (
        <SectionToolbar>
          <Input
            value={query}
            onChange={(event) => setQuery(event.currentTarget.value)}
            placeholder="Search jobs by id, target, kind, or status"
            aria-label="Search jobs"
            className="max-w-md"
          />
          <Button variant="outline" onClick={() => setDialogState({ mode: "create" })}>
            <Plus className="size-4" />
            Add Job
          </Button>
        </SectionToolbar>
      )}

      {error ? <p className="text-destructive text-xs">{error}</p> : null}

      <SectionContent>
        {hasNoJobs ? (
          <SectionEmpty
            icon={Clock3}
            title="No Clockwork Jobs"
            description="Create a job to schedule a message for an actor session."
            action={
              <Button onClick={() => setDialogState({ mode: "create" })}>
                + Add Job
              </Button>
            }
          />
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Status</TableHead>
                <TableHead>Job</TableHead>
                <TableHead>Target</TableHead>
                <TableHead>Schedule</TableHead>
                <TableHead>Next Run</TableHead>
                <TableHead>Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {isLoading ? (
                <TableRow>
                  <TableCell colSpan={6} className="text-muted-foreground text-center">
                    <span className="inline-flex items-center gap-2">
                      <LoaderCircle className="size-4 animate-spin" />
                      Loading jobs...
                    </span>
                  </TableCell>
                </TableRow>
              ) : (
                filteredJobs.map((job) => (
                  <TableRow key={job.job_id}>
                    <TableCell>
                      <Badge
                        className={
                          job.status === "active"
                            ? "border-emerald-300 bg-emerald-100 text-emerald-900"
                            : job.status === "paused"
                              ? "border-amber-300 bg-amber-100 text-amber-900"
                              : job.status === "completed"
                                ? "border-sky-300 bg-sky-100 text-sky-900"
                                : "border-rose-300 bg-rose-100 text-rose-900"
                        }
                      >
                        {job.status}
                      </Badge>
                    </TableCell>
                    <TableCell className="font-mono text-[11px]">{job.job_id}</TableCell>
                    <TableCell className="font-mono text-[11px]">
                      <div className="font-sans text-sm">
                        {actorNameById.get(job.target_actor_id) ?? job.target_actor_id}
                      </div>
                      <div className="text-muted-foreground">{job.target_session_id}</div>
                    </TableCell>
                    <TableCell className="font-mono text-[11px]">{job.kind}</TableCell>
                    <TableCell>
                      {job.next_run_at ? new Date(job.next_run_at).toLocaleString() : "-"}
                    </TableCell>
                    <TableCell className="space-x-2">
                      <Button
                        size="icon-sm"
                        variant="outline"
                        onClick={() => setDialogState({ mode: "edit", job })}
                        title="Edit job"
                        aria-label={`Edit ${job.job_id}`}
                        disabled={job.status === "cancelled" || job.status === "completed"}
                      >
                        <Pencil className="size-3.5" />
                      </Button>
                      <Button
                        size="icon-sm"
                        variant="outline"
                        onClick={() => void handlePause(job.job_id)}
                        disabled={job.status !== "active"}
                        title="Pause job"
                        aria-label={`Pause ${job.job_id}`}
                      >
                        <Pause className="size-3.5" />
                      </Button>
                      <Button
                        size="icon-sm"
                        variant="outline"
                        onClick={() => void handleResume(job.job_id)}
                        disabled={job.status !== "paused"}
                        title="Resume job"
                        aria-label={`Resume ${job.job_id}`}
                      >
                        <Play className="size-3.5" />
                      </Button>
                      <Button
                        size="icon-sm"
                        variant="outline"
                        onClick={() => void handleCancel(job.job_id)}
                        disabled={job.status === "cancelled" || job.status === "completed"}
                        title="Cancel job"
                        aria-label={`Cancel ${job.job_id}`}
                      >
                        <Trash2 className="size-3.5" />
                      </Button>
                    </TableCell>
                  </TableRow>
                ))
              )}
            </TableBody>
          </Table>
        )}
      </SectionContent>

      <AddClockworkJobForm
        open={dialogState !== null}
        onOpenChange={(open) => {
          if (!open) setDialogState(null);
        }}
        isSaving={isSaving}
        title={dialogState?.mode === "edit" ? "Edit Clockwork Job" : "Add Clockwork Job"}
        actors={actors}
        loadSessionsForActor={(actorId) => borgApi.listActorSessions(actorId, 500)}
        initialValue={formInitialValue}
        onSubmit={handleSubmit}
      />
    </Section>
  );
}
