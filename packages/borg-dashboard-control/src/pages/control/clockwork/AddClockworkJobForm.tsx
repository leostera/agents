import {
  Button,
  Calendar,
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Input,
  Label,
  Popover,
  PopoverContent,
  PopoverTrigger,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Textarea,
} from "@borg/ui";
import React from "react";

export type ClockworkJobFormInput = {
  kind: "once" | "cron";
  actorId: string;
  sessionId: string;
  messageText: string;
  runAt: string;
  cronExpression: string;
};

type AddClockworkJobFormProps = {
  open: boolean;
  isSaving: boolean;
  title: string;
  actors: { actor_id: string; name: string }[];
  loadSessionsForActor: (actorId: string) => Promise<string[]>;
  initialValue: ClockworkJobFormInput;
  onOpenChange: (open: boolean) => void;
  onSubmit: (input: ClockworkJobFormInput) => Promise<void>;
};

const EMPTY_FORM: ClockworkJobFormInput = {
  kind: "once",
  actorId: "",
  sessionId: "",
  messageText: "",
  runAt: "",
  cronExpression: "*/5 * * * *",
};

function toDatetimeLocalInput(value?: string): string {
  if (!value) return "";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  const year = String(date.getUTCFullYear());
  const month = String(date.getUTCMonth() + 1).padStart(2, "0");
  const day = String(date.getUTCDate()).padStart(2, "0");
  const hour = String(date.getUTCHours()).padStart(2, "0");
  const minute = String(date.getUTCMinutes()).padStart(2, "0");
  return `${year}-${month}-${day}T${hour}:${minute}`;
}

function toUtcDatePart(value?: string): string {
  const local = toDatetimeLocalInput(value);
  if (!local) return "";
  return local.slice(0, 10);
}

function toUtcTimePart(value?: string): string {
  const local = toDatetimeLocalInput(value);
  if (!local) return "";
  return local.slice(11, 16);
}

function composeUtcIso(datePart: string, timePart: string): string {
  const [year, month, day] = datePart.split("-").map(Number);
  const [hour, minute] = timePart.split(":").map(Number);
  if (
    !Number.isFinite(year) ||
    !Number.isFinite(month) ||
    !Number.isFinite(day) ||
    !Number.isFinite(hour) ||
    !Number.isFinite(minute)
  ) {
    return "";
  }
  return new Date(
    Date.UTC(year, month - 1, day, hour, minute, 0, 0)
  ).toISOString();
}

function toDateFromUtcPart(datePart: string): Date | undefined {
  if (!datePart) return undefined;
  const date = new Date(`${datePart}T00:00:00Z`);
  return Number.isNaN(date.getTime()) ? undefined : date;
}

export function AddClockworkJobForm({
  open,
  isSaving,
  title,
  actors,
  loadSessionsForActor,
  initialValue,
  onOpenChange,
  onSubmit,
}: AddClockworkJobFormProps) {
  const [form, setForm] = React.useState<ClockworkJobFormInput>(EMPTY_FORM);
  const [error, setError] = React.useState<string | null>(null);
  const [availableSessions, setAvailableSessions] = React.useState<string[]>(
    []
  );
  const [isLoadingSessions, setIsLoadingSessions] = React.useState(false);
  const selectedDatePart = toUtcDatePart(form.runAt);
  const selectedTimePart = toUtcTimePart(form.runAt) || "00:00";
  const selectedDate = toDateFromUtcPart(selectedDatePart);

  React.useEffect(() => {
    if (!open) {
      setError(null);
      return;
    }
    setForm({
      ...initialValue,
      runAt: toDatetimeLocalInput(initialValue.runAt),
    });
  }, [open, initialValue]);

  React.useEffect(() => {
    if (!open || !form.actorId.trim()) {
      setAvailableSessions([]);
      return;
    }
    let isActive = true;
    setIsLoadingSessions(true);
    void loadSessionsForActor(form.actorId)
      .then((sessions) => {
        if (!isActive) return;
        setAvailableSessions(sessions);
        if (!sessions.includes(form.sessionId)) {
          setForm((current) => ({
            ...current,
            sessionId: sessions[0] ?? "",
          }));
        }
      })
      .catch(() => {
        if (!isActive) return;
        setAvailableSessions([]);
      })
      .finally(() => {
        if (!isActive) return;
        setIsLoadingSessions(false);
      });
    return () => {
      isActive = false;
    };
  }, [form.actorId, form.sessionId, loadSessionsForActor, open]);

  const handleSubmit = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setError(null);

    if (!form.actorId.trim()) {
      setError("Actor is required");
      return;
    }
    if (!form.sessionId.trim()) {
      setError("Session is required");
      return;
    }
    if (!form.messageText.trim()) {
      setError("Message text is required");
      return;
    }

    if (form.kind === "once") {
      if (!form.runAt.trim()) {
        setError("Run time is required for one-shot jobs");
        return;
      }
    } else if (!form.cronExpression.trim()) {
      setError("Cron expression is required for cron jobs");
      return;
    }

    await onSubmit(form);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>
        <form className="space-y-3" onSubmit={handleSubmit}>
          <div className="space-y-1">
            <div className="space-y-1">
              <Label>Kind</Label>
              <Select
                value={form.kind}
                onValueChange={(kind) =>
                  setForm((current) => ({
                    ...current,
                    kind: kind as "once" | "cron",
                  }))
                }
              >
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="once">One-shot</SelectItem>
                  <SelectItem value="cron">Cron</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>

          {form.kind === "once" ? (
            <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
              <div className="space-y-1">
                <Label>Select date (UTC)</Label>
                <Popover>
                  <PopoverTrigger asChild>
                    <Button
                      variant="outline"
                      className="w-full justify-start text-left font-normal"
                    >
                      {selectedDatePart || "Pick date"}
                    </Button>
                  </PopoverTrigger>
                  <PopoverContent className="w-auto p-0" align="start">
                    <Calendar
                      mode="single"
                      selected={selectedDate}
                      onSelect={(picked) => {
                        if (!picked) return;
                        const y = String(picked.getUTCFullYear());
                        const m = String(picked.getUTCMonth() + 1).padStart(
                          2,
                          "0"
                        );
                        const d = String(picked.getUTCDate()).padStart(2, "0");
                        const nextDatePart = `${y}-${m}-${d}`;
                        const nextRunAt = composeUtcIso(
                          nextDatePart,
                          selectedTimePart
                        );
                        setForm((current) => ({
                          ...current,
                          runAt: nextRunAt,
                        }));
                      }}
                    />
                  </PopoverContent>
                </Popover>
              </div>

              <div className="space-y-1">
                <Label htmlFor="clockwork-run-time">Select time (UTC)</Label>
                <Input
                  id="clockwork-run-time"
                  type="time"
                  value={selectedTimePart}
                  onChange={(event) => {
                    const nextTimePart = event.currentTarget.value || "00:00";
                    const nextDatePart =
                      selectedDatePart ||
                      toUtcDatePart(new Date().toISOString());
                    const nextRunAt = nextDatePart
                      ? composeUtcIso(nextDatePart, nextTimePart)
                      : "";
                    setForm((current) => ({
                      ...current,
                      runAt: nextRunAt,
                    }));
                  }}
                  required
                />
              </div>
            </div>
          ) : (
            <div className="space-y-1">
              <Label htmlFor="clockwork-cron">Cron expression (UTC)</Label>
              <Input
                id="clockwork-cron"
                value={form.cronExpression}
                onChange={(event) =>
                  setForm((current) => ({
                    ...current,
                    cronExpression: event.currentTarget.value,
                  }))
                }
                placeholder="*/5 * * * *"
                required
              />
            </div>
          )}

          <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
            <div className="space-y-1">
              <Label>Target Actor</Label>
              <Select
                value={form.actorId}
                onValueChange={(actorId) =>
                  setForm((current) => ({
                    ...current,
                    actorId,
                    sessionId: "",
                  }))
                }
              >
                <SelectTrigger>
                  <SelectValue placeholder="Select actor" />
                </SelectTrigger>
                <SelectContent>
                  {actors.map((actor) => (
                    <SelectItem key={actor.actor_id} value={actor.actor_id}>
                      {actor.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>

            <div className="space-y-1">
              <Label>Target Session</Label>
              <Select
                value={form.sessionId}
                onValueChange={(sessionId) =>
                  setForm((current) => ({
                    ...current,
                    sessionId,
                  }))
                }
                disabled={!form.actorId || isLoadingSessions}
              >
                <SelectTrigger>
                  <SelectValue
                    placeholder={
                      !form.actorId
                        ? "Select actor first"
                        : isLoadingSessions
                          ? "Loading sessions..."
                          : "Select session"
                    }
                  />
                </SelectTrigger>
                <SelectContent>
                  {availableSessions.map((sessionId) => (
                    <SelectItem key={sessionId} value={sessionId}>
                      {sessionId}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          </div>

          <div className="space-y-1">
            <Label htmlFor="clockwork-message">Message body</Label>
            <Textarea
              id="clockwork-message"
              rows={6}
              value={form.messageText}
              onChange={(event) =>
                setForm((current) => ({
                  ...current,
                  messageText: event.currentTarget.value,
                }))
              }
              placeholder="Write the message payload text that Clockwork will deliver"
              required
            />
          </div>

          {error ? <p className="text-destructive text-xs">{error}</p> : null}

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={isSaving}>
              {isSaving ? "Saving..." : "Save Job"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
