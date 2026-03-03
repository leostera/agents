import {
  type ActorRecord,
  type DevModeProjectRecord,
  type DevModeSpecRecord,
  createBorgApiClient,
} from "@borg/api";
import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Input,
  Label,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
  Textarea,
} from "@borg/ui";
import { Archive, GitFork, LoaderCircle, Pencil, Plus } from "lucide-react";
import React from "react";
import {
  Section,
  SectionContent,
  SectionEmpty,
  SectionToolbar,
} from "../../../components/Section";

const borgApi = createBorgApiClient();

function newDevModeUri(kind: "project" | "spec"): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return `devmode:${kind}:${crypto.randomUUID()}`;
  }
  return `devmode:${kind}:${Date.now()}`;
}

function useDevModeData() {
  const [projects, setProjects] = React.useState<DevModeProjectRecord[]>([]);
  const [specs, setSpecs] = React.useState<DevModeSpecRecord[]>([]);
  const [actors, setActors] = React.useState<ActorRecord[]>([]);
  const [isLoading, setIsLoading] = React.useState(true);
  const [error, setError] = React.useState<string | null>(null);

  const load = React.useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const [projectRows, specRows, actorRows] = await Promise.all([
        borgApi.listDevModeProjects(500),
        borgApi.listDevModeSpecs({ limit: 500 }),
        borgApi.listActors(500),
      ]);
      setProjects(projectRows);
      setSpecs(specRows);
      setActors(actorRows);
    } catch (loadError) {
      setProjects([]);
      setSpecs([]);
      setActors([]);
      setError(
        loadError instanceof Error ? loadError.message : "Unable to load DevMode"
      );
    } finally {
      setIsLoading(false);
    }
  }, []);

  React.useEffect(() => {
    void load();
  }, [load]);

  return {
    projects,
    specs,
    actors,
    isLoading,
    error,
    setError,
    reload: load,
  };
}

export function DevModeProjectsPage() {
  const { projects, isLoading, error, setError, reload } = useDevModeData();
  const [isDialogOpen, setIsDialogOpen] = React.useState(false);
  const [isSaving, setIsSaving] = React.useState(false);
  const [editingProjectId, setEditingProjectId] = React.useState<string | null>(null);
  const [projectRootPath, setProjectRootPath] = React.useState("");
  const [projectDescription, setProjectDescription] = React.useState("");
  const hasNoProjects = !isLoading && projects.length === 0;

  const handleSaveProject = React.useCallback(async () => {
    const rootPath = projectRootPath.trim();
    if (!rootPath) {
      setError("Project root path is required.");
      return;
    }
    setIsSaving(true);
    setError(null);
    try {
      await borgApi.upsertDevModeProject(editingProjectId ?? newDevModeUri("project"), {
        name: projectDescription.trim() || "Untitled Project",
        rootPath,
        description: projectDescription.trim(),
        status: "ONGOING",
      });
      setProjectRootPath("");
      setProjectDescription("");
      setEditingProjectId(null);
      setIsDialogOpen(false);
      await reload();
    } catch (saveError) {
      setError(
        saveError instanceof Error ? saveError.message : "Unable to save project"
      );
    } finally {
      setIsSaving(false);
    }
  }, [editingProjectId, projectDescription, projectRootPath, reload, setError]);

  const handleEditProject = React.useCallback((project: DevModeProjectRecord) => {
    setEditingProjectId(project.project_id);
    setProjectRootPath(project.root_path);
    setProjectDescription(project.description ?? "");
    setIsDialogOpen(true);
  }, []);

  const handleArchiveProject = React.useCallback(
    async (project: DevModeProjectRecord) => {
      if (project.status === "ARCHIVED") return;
      setIsSaving(true);
      setError(null);
      try {
        await borgApi.upsertDevModeProject(project.project_id, {
          name: project.description?.trim() || project.project_id,
          rootPath: project.root_path,
          description: project.description,
          status: "ARCHIVED",
        });
        await reload();
      } catch (archiveError) {
        setError(
          archiveError instanceof Error
            ? archiveError.message
            : "Unable to archive project"
        );
      } finally {
        setIsSaving(false);
      }
    },
    [reload, setError]
  );

  const handleBrowseForProject = React.useCallback(async () => {
    try {
      const picker = (window as unknown as {
        showDirectoryPicker?: () => Promise<{
          name: string;
          values?: () => AsyncIterable<{ kind: string; getFile?: () => Promise<File> }>;
          path?: string;
        }>;
      }).showDirectoryPicker;
      if (!picker) {
        setError("Folder picker is unavailable in this environment. Paste path manually.");
        return;
      }

      const handle = await picker();
      const handlePath = handle.path?.trim();
      if (handlePath) {
        setProjectRootPath(handlePath);
        setError(null);
        return;
      }

      // Some desktop runtimes expose file.path on picker file handles; use it when available.
      if (handle.values) {
        for await (const entry of handle.values()) {
          if (entry.kind !== "file" || !entry.getFile) continue;
          const file = (await entry.getFile()) as File & { path?: string };
          const filePath = file.path?.trim();
          if (!filePath) break;
          const normalized = filePath.replace(/\\/g, "/");
          const parts = normalized.split("/");
          if (parts.length > 1) {
            setProjectRootPath(parts.slice(0, -1).join("/"));
            setError(null);
            return;
          }
        }
      }

      setProjectRootPath(handle.name ?? "");
      setError(
        "Selected folder path is unavailable in this environment. Verify or paste the absolute path."
      );
    } catch (pickerError) {
      if (pickerError instanceof Error && pickerError.name === "AbortError") {
        return;
      }
      setError(
        pickerError instanceof Error ? pickerError.message : "Unable to select folder."
      );
    }
  }, [setError]);

  return (
    <Section className="gap-4">
      {hasNoProjects ? null : (
        <SectionToolbar>
          <Button
            variant="outline"
            onClick={() => {
              setEditingProjectId(null);
              setProjectRootPath("");
              setProjectDescription("");
              setIsDialogOpen(true);
            }}
          >
            <Plus className="size-4" />
            Add Project
          </Button>
        </SectionToolbar>
      )}

      {error ? <p className="text-destructive text-xs">{error}</p> : null}

      <SectionContent>
        {isLoading ? (
          <p className="text-muted-foreground inline-flex items-center gap-2 text-sm">
            <LoaderCircle className="size-4 animate-spin" />
            Loading projects...
          </p>
        ) : projects.length === 0 ? (
          <SectionEmpty
            icon={GitFork}
            title="No Projects Yet"
            description="Add a repository root path to start using DevMode."
            action={
              <Button
                onClick={() => {
                  setEditingProjectId(null);
                  setProjectRootPath("");
                  setProjectDescription("");
                  setIsDialogOpen(true);
                }}
              >
                + Add Project
              </Button>
            }
          />
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Status</TableHead>
                <TableHead>Project</TableHead>
                <TableHead>Description</TableHead>
                <TableHead>Root Path</TableHead>
                <TableHead>Updated</TableHead>
                <TableHead>Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {projects.map((project) => (
                <TableRow key={project.project_id}>
                  <TableCell>
                    <span
                      className={`inline-block size-2.5 rounded-full ${
                        project.status === "ARCHIVED"
                          ? "bg-slate-400"
                          : "bg-emerald-500"
                      }`}
                      title={project.status === "ARCHIVED" ? "Archived" : "Ongoing"}
                      aria-label={project.status === "ARCHIVED" ? "Archived" : "Ongoing"}
                    />
                  </TableCell>
                  <TableCell className="font-mono text-xs">{project.project_id}</TableCell>
                  <TableCell className="max-w-[320px] truncate">
                    {project.description || "—"}
                  </TableCell>
                  <TableCell className="font-mono text-xs">{project.root_path}</TableCell>
                  <TableCell>{new Date(project.updated_at).toLocaleString()}</TableCell>
                  <TableCell className="space-x-2">
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => handleEditProject(project)}
                    >
                      <Pencil className="size-3.5" />
                      Edit
                    </Button>
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={project.status === "ARCHIVED" || isSaving}
                      onClick={() => void handleArchiveProject(project)}
                    >
                      <Archive className="size-3.5" />
                      Archive
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </SectionContent>

      <Dialog open={isDialogOpen} onOpenChange={setIsDialogOpen}>
        <DialogContent className="sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>
              {editingProjectId ? "Edit Project" : "Add Project"}
            </DialogTitle>
            <DialogDescription>
              Add a local repository root path for DevMode.
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-2">
            <Label htmlFor="devmode-project-root-path">Root Path</Label>
            <div className="grid grid-cols-[1fr_auto] gap-2">
              <Input
                id="devmode-project-root-path"
                value={projectRootPath}
                onChange={(event) => setProjectRootPath(event.currentTarget.value)}
                placeholder="/absolute/path/to/repo"
              />
              <Button
                type="button"
                variant="outline"
                onClick={() => void handleBrowseForProject()}
                disabled={isSaving}
              >
                Select Folder
              </Button>
            </div>
          </div>
          <div className="space-y-2">
            <Label htmlFor="devmode-project-description">Description</Label>
            <Textarea
              id="devmode-project-description"
              value={projectDescription}
              onChange={(event) => setProjectDescription(event.currentTarget.value)}
              placeholder="What this project is about"
              rows={3}
            />
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setIsDialogOpen(false)}>
              Cancel
            </Button>
            <Button onClick={() => void handleSaveProject()} disabled={isSaving}>
              {isSaving ? (
                <span className="inline-flex items-center gap-2">
                  <LoaderCircle className="size-4 animate-spin" />
                  Saving...
                </span>
              ) : (
                "Save Project"
              )}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </Section>
  );
}

export function DevModeSpecsPage() {
  const { projects, specs, actors, isLoading, error, setError, reload } =
    useDevModeData();
  const [isSaving, setIsSaving] = React.useState(false);
  const [specProjectId, setSpecProjectId] = React.useState("");
  const [specTitle, setSpecTitle] = React.useState("");
  const [specBody, setSpecBody] = React.useState("");
  const [sessionUri, setSessionUri] = React.useState("borg:session:devmode-bootstrap");
  const [creatorActorId, setCreatorActorId] = React.useState("");
  const [assigneeActorId, setAssigneeActorId] = React.useState("");

  React.useEffect(() => {
    if (!specProjectId && projects.length > 0) {
      setSpecProjectId(projects[0].project_id);
    }
  }, [projects, specProjectId]);

  React.useEffect(() => {
    if (!creatorActorId && actors.length > 0) {
      setCreatorActorId(actors[0].actor_id);
    }
    if (!assigneeActorId && actors.length > 0) {
      setAssigneeActorId(actors[0].actor_id);
    }
  }, [actors, assigneeActorId, creatorActorId]);

  const handleCreateSpec = React.useCallback(async () => {
    const projectId = specProjectId.trim();
    const title = specTitle.trim();
    const body = specBody.trim();
    if (!projectId) {
      setError("Spec project is required.");
      return;
    }
    if (!title) {
      setError("Spec title is required.");
      return;
    }
    if (!body) {
      setError("Spec body is required.");
      return;
    }
    setIsSaving(true);
    setError(null);
    try {
      await borgApi.upsertDevModeSpec(newDevModeUri("spec"), {
        projectId,
        title,
        body,
        status: "DRAFT",
      });
      setSpecTitle("");
      setSpecBody("");
      await reload();
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : "Unable to create spec");
    } finally {
      setIsSaving(false);
    }
  }, [reload, setError, specBody, specProjectId, specTitle]);

  const handleMaterialize = React.useCallback(
    async (specId: string) => {
      const creator = creatorActorId.trim();
      if (!creator) {
        setError("Creator actor is required.");
        return;
      }
      setIsSaving(true);
      setError(null);
      try {
        await borgApi.materializeDevModeSpec(specId, {
          sessionUri: sessionUri.trim(),
          creatorActorId: creator,
          assigneeActorId: assigneeActorId.trim() || creator,
        });
        await reload();
      } catch (materializeError) {
        setError(
          materializeError instanceof Error
            ? materializeError.message
            : "Unable to materialize spec"
        );
      } finally {
        setIsSaving(false);
      }
    },
    [assigneeActorId, creatorActorId, reload, sessionUri, setError]
  );

  return (
    <Section className="gap-4">
      <section className="grid gap-3 rounded-lg border p-3 md:grid-cols-2">
        <div className="space-y-2">
          <Label>Project</Label>
          <Select value={specProjectId} onValueChange={setSpecProjectId}>
            <SelectTrigger>
              <SelectValue placeholder="Select project" />
            </SelectTrigger>
            <SelectContent>
              {projects.map((project) => (
                <SelectItem key={project.project_id} value={project.project_id}>
                  {project.project_id}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Input
            value={specTitle}
            onChange={(event) => setSpecTitle(event.currentTarget.value)}
            placeholder="Spec title"
          />
          <Textarea
            value={specBody}
            onChange={(event) => setSpecBody(event.currentTarget.value)}
            placeholder="Spec body"
            rows={4}
          />
          <Button
            variant="outline"
            onClick={() => void handleCreateSpec()}
            disabled={isSaving || projects.length === 0}
          >
            + Add Spec
          </Button>
        </div>
        <div className="space-y-2">
          <Label>Materialization Context</Label>
          <Input
            value={sessionUri}
            onChange={(event) => setSessionUri(event.currentTarget.value)}
            placeholder="borg:session:..."
          />
          <Select value={creatorActorId} onValueChange={setCreatorActorId}>
            <SelectTrigger>
              <SelectValue placeholder="Creator actor" />
            </SelectTrigger>
            <SelectContent>
              {actors.map((actor) => (
                <SelectItem key={actor.actor_id} value={actor.actor_id}>
                  {actor.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Select value={assigneeActorId} onValueChange={setAssigneeActorId}>
            <SelectTrigger>
              <SelectValue placeholder="Assignee actor" />
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
      </section>

      {error ? <p className="text-destructive text-xs">{error}</p> : null}

      <SectionContent>
        {isLoading ? (
          <p className="text-muted-foreground inline-flex items-center gap-2 text-sm">
            <LoaderCircle className="size-4 animate-spin" />
            Loading specs...
          </p>
        ) : specs.length === 0 ? (
          <SectionEmpty
            icon={GitFork}
            title="No Specs Yet"
            description="Create a spec and materialize it into TaskGraph."
          />
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Title</TableHead>
                <TableHead>Project</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Root Task</TableHead>
                <TableHead>Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {specs.map((spec) => (
                <TableRow key={spec.spec_id}>
                  <TableCell>{spec.title}</TableCell>
                  <TableCell className="font-mono text-xs">{spec.project_id}</TableCell>
                  <TableCell>{spec.status}</TableCell>
                  <TableCell className="font-mono text-xs">
                    {spec.root_task_uri ?? "—"}
                  </TableCell>
                  <TableCell>
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={isSaving || spec.status === "TASK_GRAPHED"}
                      onClick={() => void handleMaterialize(spec.spec_id)}
                    >
                      Materialize
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </SectionContent>
    </Section>
  );
}

export function DevModeWorkersPage() {
  return (
    <Section className="gap-4">
      <SectionContent>
        <SectionEmpty
          icon={GitFork}
          title="Workers Coming Next"
          description="Worker lifecycle, assignment, and publish orchestration UI will live here."
        />
      </SectionContent>
    </Section>
  );
}
