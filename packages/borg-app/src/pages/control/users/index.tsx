import { createBorgApiClient, type UserRecord } from "@borg/api";
import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
  Input,
  Label,
  Link,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
  Textarea,
} from "@borg/ui";
import { LoaderCircle, Plus, Power, User } from "lucide-react";
import React from "react";

const borgApi = createBorgApiClient();

type UserFormState = {
  userKey: string;
  profileJson: string;
};

const DEFAULT_FORM: UserFormState = {
  userKey: "",
  profileJson: "{}",
};

function parseProfileJson(raw: string): Record<string, unknown> {
  const value = raw.trim();
  if (!value) return {};
  const parsed = JSON.parse(value);
  if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
    return parsed as Record<string, unknown>;
  }
  throw new Error("Profile must be a JSON object");
}

export function UsersPage() {
  const [users, setUsers] = React.useState<UserRecord[]>([]);
  const [query, setQuery] = React.useState(
    () => new URLSearchParams(window.location.search).get("q") ?? ""
  );
  const [isLoading, setIsLoading] = React.useState(true);
  const [error, setError] = React.useState<string | null>(null);
  const [isDialogOpen, setIsDialogOpen] = React.useState(false);
  const [isSaving, setIsSaving] = React.useState(false);
  const [form, setForm] = React.useState<UserFormState>(DEFAULT_FORM);

  const loadUsers = React.useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const rows = await borgApi.listUsers(500);
      setUsers(rows);
    } catch (loadError) {
      setUsers([]);
      setError(
        loadError instanceof Error ? loadError.message : "Unable to load users"
      );
    } finally {
      setIsLoading(false);
    }
  }, []);

  React.useEffect(() => {
    void loadUsers();
  }, [loadUsers]);

  React.useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    if (query.trim()) {
      params.set("q", query.trim());
    } else {
      params.delete("q");
    }
    const paramsString = params.toString();
    const url = paramsString
      ? `/control/users?${paramsString}`
      : "/control/users";
    window.history.replaceState(null, "", url);
  }, [query]);

  const filteredUsers = React.useMemo(() => {
    const term = query.trim().toLowerCase();
    if (!term) return users;
    return users.filter((user) => {
      const haystack = [user.user_key, JSON.stringify(user.profile)]
        .join(" ")
        .toLowerCase();
      return haystack.includes(term);
    });
  }, [users, query]);

  const handleCreateUser = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setError(null);

    let profile: Record<string, unknown>;
    try {
      profile = parseProfileJson(form.profileJson);
    } catch (parseError) {
      setError(
        parseError instanceof Error
          ? parseError.message
          : "Invalid profile JSON"
      );
      return;
    }

    setIsSaving(true);
    try {
      await borgApi.upsertUser(form.userKey.trim(), profile);
      setForm(DEFAULT_FORM);
      setIsDialogOpen(false);
      await loadUsers();
    } catch (saveError) {
      setError(
        saveError instanceof Error ? saveError.message : "Unable to create user"
      );
    } finally {
      setIsSaving(false);
    }
  };

  const handleDisableUser = async (userKey: string) => {
    setError(null);
    try {
      await borgApi.deleteUser(userKey, { ignoreNotFound: true });
      await loadUsers();
    } catch (deleteError) {
      setError(
        deleteError instanceof Error
          ? deleteError.message
          : "Unable to disable user"
      );
    }
  };

  return (
    <section className="space-y-4">
      <section className="flex flex-wrap items-center gap-2">
        <Input
          value={query}
          onChange={(event) => setQuery(event.currentTarget.value)}
          placeholder="Search users by key or profile"
          aria-label="Search users"
          className="max-w-md"
        />
        <Button variant="outline" onClick={() => setIsDialogOpen(true)}>
          <Plus className="size-4" />
          Add User
        </Button>
      </section>

      {error ? <p className="text-destructive text-xs">{error}</p> : null}

      {!isLoading && filteredUsers.length === 0 ? (
        <Empty className="border">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <User />
            </EmptyMedia>
            <EmptyTitle>No Users Found</EmptyTitle>
            <EmptyDescription>
              Create your first user profile to start sessions.
            </EmptyDescription>
          </EmptyHeader>
          <EmptyContent className="flex-row justify-center">
            <Button onClick={() => setIsDialogOpen(true)}>+ Add User</Button>
          </EmptyContent>
        </Empty>
      ) : (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>User</TableHead>
              <TableHead>Profile</TableHead>
              <TableHead>Updated</TableHead>
              <TableHead>Actions</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {isLoading ? (
              <TableRow>
                <TableCell
                  colSpan={4}
                  className="text-muted-foreground text-center"
                >
                  <span className="inline-flex items-center gap-2">
                    <LoaderCircle className="size-4 animate-spin" />
                    Loading users...
                  </span>
                </TableCell>
              </TableRow>
            ) : (
              filteredUsers.map((user) => (
                <TableRow key={user.user_key}>
                  <TableCell className="font-mono text-[11px]">
                    <Link href={`/control/users/${user.user_key}`}>
                      {user.user_key}
                    </Link>
                  </TableCell>
                  <TableCell className="font-mono text-[11px]">
                    {JSON.stringify(user.profile)}
                  </TableCell>
                  <TableCell>
                    {new Date(user.updated_at).toLocaleString()}
                  </TableCell>
                  <TableCell>
                    <Button
                      size="icon-sm"
                      variant="outline"
                      onClick={() => void handleDisableUser(user.user_key)}
                      aria-label={`Disable ${user.user_key}`}
                      title="Disable user"
                    >
                      <Power className="size-3.5" />
                    </Button>
                  </TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </Table>
      )}

      <Dialog open={isDialogOpen} onOpenChange={setIsDialogOpen}>
        <DialogContent className="sm:max-w-xl">
          <DialogHeader>
            <DialogTitle>Add User</DialogTitle>
            <DialogDescription>
              Create a new user with an initial profile.
            </DialogDescription>
          </DialogHeader>

          <form className="space-y-3" onSubmit={handleCreateUser}>
            <div className="space-y-1">
              <Label htmlFor="user-key">User Key (URI)</Label>
              <Input
                id="user-key"
                value={form.userKey}
                onChange={(event) =>
                  setForm((current) => ({
                    ...current,
                    userKey: event.currentTarget.value,
                  }))
                }
                placeholder="borg:user:alice"
                required
              />
            </div>
            <div className="space-y-1">
              <Label htmlFor="user-profile">Profile (JSON)</Label>
              <Textarea
                id="user-profile"
                value={form.profileJson}
                onChange={(event) =>
                  setForm((current) => ({
                    ...current,
                    profileJson: event.currentTarget.value,
                  }))
                }
                rows={8}
              />
            </div>
            <DialogFooter>
              <Button type="submit" disabled={isSaving}>
                {isSaving ? (
                  <LoaderCircle className="size-4 animate-spin" />
                ) : null}
                Save User
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </section>
  );
}
