import { createBorgApiClient } from "@borg/api";
import { Badge, Card, CardContent } from "@borg/ui";
import { LoaderCircle } from "lucide-react";
import React from "react";

type MemoryEntity = {
  entity_id: string;
  entity_type: string;
  label: string;
  props?: Record<string, unknown>;
};

type FieldSchema = {
  name: string;
  description: string;
  domain: string;
  range: string;
  arity: string;
  optionality: string;
  examples: string[];
};

const borgApi = createBorgApiClient();
const URI_RE = /^[a-z][a-z0-9+.-]*:[^:\s]+:[^:\s]+$/i;

function namespaceFromEntityId(entityId: string): string {
  const [head] = entityId.split(":");
  return head?.trim() || "unknown";
}

function inferValueType(value: unknown): string {
  if (value === null || value === undefined) return "null";
  if (Array.isArray(value)) return "array";
  if (typeof value === "string") {
    if (URI_RE.test(value)) return "uri-ref";
    return "string";
  }
  if (typeof value === "number") return "number";
  if (typeof value === "boolean") return "boolean";
  if (typeof value === "object") return "object";
  return typeof value;
}

function deriveFieldSchema(
  kindEntities: MemoryEntity[],
  fieldName: string
): FieldSchema {
  const values = kindEntities
    .map((entity) => entity.props?.[fieldName])
    .filter((value) => value !== undefined);

  const typeSet = new Set<string>();
  const exampleSet = new Set<string>();
  let listCount = 0;

  for (const value of values) {
    const valueType = inferValueType(value);
    typeSet.add(valueType);
    if (valueType === "array") listCount += 1;

    const sample = typeof value === "string" ? value : JSON.stringify(value);
    if (sample && sample.length > 0) exampleSet.add(sample.slice(0, 80));
  }

  const presenceRatio = `${values.length}/${kindEntities.length}`;
  const optionality =
    values.length === kindEntities.length
      ? "required"
      : `optional (${presenceRatio})`;
  const arity =
    listCount === 0
      ? "one"
      : listCount === values.length
        ? "many"
        : `mixed (${listCount}/${values.length} lists)`;
  const typeList = Array.from(typeSet);
  const domain = typeList.length > 0 ? typeList.join(" | ") : "unknown";
  const range = typeSet.has("uri-ref") ? "entity references" : domain;
  const description = `Observed on ${values.length} of ${kindEntities.length} ${kindEntities[0]?.entity_type || "entity"} entities`;

  return {
    name: fieldName,
    description,
    domain,
    range,
    arity,
    optionality,
    examples: Array.from(exampleSet).slice(0, 4),
  };
}

export function MemoryShapesPage() {
  const [isLoading, setIsLoading] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [entities, setEntities] = React.useState<MemoryEntity[]>([]);

  const [selectedNamespace, setSelectedNamespace] = React.useState("");
  const [selectedKind, setSelectedKind] = React.useState("");
  const [selectedField, setSelectedField] = React.useState("");

  React.useEffect(() => {
    const load = async () => {
      setIsLoading(true);
      setError(null);
      try {
        const rows = (await borgApi.searchMemory({
          q: "",
          limit: 500,
        })) as MemoryEntity[];
        setEntities(rows);
      } catch (loadError) {
        setEntities([]);
        setError(
          loadError instanceof Error
            ? loadError.message
            : "Unable to load shapes"
        );
      } finally {
        setIsLoading(false);
      }
    };

    void load();
  }, []);

  const namespaceMap = React.useMemo(() => {
    const map = new Map<string, MemoryEntity[]>();
    for (const entity of entities) {
      const namespace = namespaceFromEntityId(entity.entity_id);
      const bucket = map.get(namespace) ?? [];
      bucket.push(entity);
      map.set(namespace, bucket);
    }
    return map;
  }, [entities]);

  const namespaces = React.useMemo(
    () => Array.from(namespaceMap.keys()).sort((a, b) => a.localeCompare(b)),
    [namespaceMap]
  );

  React.useEffect(() => {
    if (!selectedNamespace && namespaces.length > 0)
      setSelectedNamespace(namespaces[0]);
  }, [namespaces, selectedNamespace]);

  const kindMap = React.useMemo(() => {
    const map = new Map<string, MemoryEntity[]>();
    const nsEntities = namespaceMap.get(selectedNamespace) ?? [];
    for (const entity of nsEntities) {
      const kind = entity.entity_type?.trim() || "unknown";
      const bucket = map.get(kind) ?? [];
      bucket.push(entity);
      map.set(kind, bucket);
    }
    return map;
  }, [namespaceMap, selectedNamespace]);

  const kinds = React.useMemo(
    () => Array.from(kindMap.keys()).sort((a, b) => a.localeCompare(b)),
    [kindMap]
  );

  React.useEffect(() => {
    if (!selectedKind || !kindMap.has(selectedKind)) {
      setSelectedKind(kinds[0] ?? "");
      setSelectedField("");
    }
  }, [kinds, kindMap, selectedKind]);

  const fields = React.useMemo(() => {
    const fieldSet = new Set<string>();
    const kindEntities = kindMap.get(selectedKind) ?? [];
    for (const entity of kindEntities) {
      Object.keys(entity.props ?? {}).forEach((field) => fieldSet.add(field));
    }
    return Array.from(fieldSet).sort((a, b) => a.localeCompare(b));
  }, [kindMap, selectedKind]);

  React.useEffect(() => {
    if (!selectedField || !fields.includes(selectedField))
      setSelectedField(fields[0] ?? "");
  }, [fields, selectedField]);

  const selectedSchema = React.useMemo(() => {
    if (!selectedKind || !selectedField) return null;
    const kindEntities = kindMap.get(selectedKind) ?? [];
    if (kindEntities.length === 0) return null;
    return deriveFieldSchema(kindEntities, selectedField);
  }, [kindMap, selectedKind, selectedField]);

  return (
    <section className="space-y-4">
      <p className="text-muted-foreground text-xs">
        Shapes explorer: click namespace, then kind, then field to inspect
        schema metadata.
      </p>

      {isLoading ? (
        <p className="text-muted-foreground inline-flex items-center gap-2 text-xs">
          <LoaderCircle className="size-4 animate-spin" />
          Building shape index...
        </p>
      ) : null}

      {error ? <p className="text-destructive text-xs">{error}</p> : null}

      <div className="grid gap-3 xl:grid-cols-4 lg:grid-cols-3 md:grid-cols-2">
        <Card>
          <CardContent className="p-2">
            <p className="mb-2 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
              Namespace
            </p>
            <div className="max-h-[30rem] space-y-1 overflow-auto">
              {namespaces.map((namespace) => (
                <button
                  key={namespace}
                  type="button"
                  onClick={() => {
                    setSelectedNamespace(namespace);
                    setSelectedKind("");
                    setSelectedField("");
                  }}
                  className={`w-full rounded-md border px-2.5 py-2 text-left text-sm ${
                    selectedNamespace === namespace
                      ? "border-primary bg-muted/40"
                      : "border-border/60"
                  }`}
                >
                  <div className="flex items-center justify-between gap-2">
                    <span className="truncate font-medium">{namespace}</span>
                    <Badge variant="secondary">
                      {namespaceMap.get(namespace)?.length ?? 0}
                    </Badge>
                  </div>
                </button>
              ))}
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="p-2">
            <p className="mb-2 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
              Kind
            </p>
            <div className="max-h-[30rem] space-y-1 overflow-auto">
              {kinds.map((kind) => (
                <button
                  key={kind}
                  type="button"
                  onClick={() => {
                    setSelectedKind(kind);
                    setSelectedField("");
                  }}
                  className={`w-full rounded-md border px-2.5 py-2 text-left text-sm ${
                    selectedKind === kind
                      ? "border-primary bg-muted/40"
                      : "border-border/60"
                  }`}
                >
                  <div className="flex items-center justify-between gap-2">
                    <span className="truncate font-medium">{kind}</span>
                    <Badge variant="secondary">
                      {kindMap.get(kind)?.length ?? 0}
                    </Badge>
                  </div>
                </button>
              ))}
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="p-2">
            <p className="mb-2 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
              Field
            </p>
            <div className="max-h-[30rem] space-y-1 overflow-auto">
              {fields.map((field) => (
                <button
                  key={field}
                  type="button"
                  onClick={() => setSelectedField(field)}
                  className={`w-full rounded-md border px-2.5 py-2 text-left text-sm ${
                    selectedField === field
                      ? "border-primary bg-muted/40"
                      : "border-border/60"
                  }`}
                >
                  <span className="truncate font-mono text-xs">{field}</span>
                </button>
              ))}
            </div>
          </CardContent>
        </Card>

        <Card className="md:col-span-2 lg:col-span-1">
          <CardContent className="space-y-2 p-3">
            <p className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
              Field Schema
            </p>
            {selectedSchema ? (
              <>
                <p className="font-mono text-sm font-medium">
                  {selectedSchema.name}
                </p>
                <p className="text-muted-foreground text-xs">
                  {selectedSchema.description}
                </p>
                <div className="grid gap-1 text-xs">
                  <p>
                    <span className="text-muted-foreground">Domain:</span>{" "}
                    {selectedSchema.domain}
                  </p>
                  <p>
                    <span className="text-muted-foreground">Range:</span>{" "}
                    {selectedSchema.range}
                  </p>
                  <p>
                    <span className="text-muted-foreground">Arity:</span>{" "}
                    {selectedSchema.arity}
                  </p>
                  <p>
                    <span className="text-muted-foreground">Optionality:</span>{" "}
                    {selectedSchema.optionality}
                  </p>
                </div>
                {selectedSchema.examples.length > 0 ? (
                  <div>
                    <p className="mb-1 text-xs text-muted-foreground">
                      Examples
                    </p>
                    <div className="space-y-1">
                      {selectedSchema.examples.map((example) => (
                        <pre
                          key={example}
                          className="overflow-auto rounded-md border border-border/60 bg-muted/30 px-2 py-1 text-[10px]"
                        >
                          {example}
                        </pre>
                      ))}
                    </div>
                  </div>
                ) : null}
              </>
            ) : (
              <p className="text-muted-foreground text-sm">
                Choose a field to inspect schema.
              </p>
            )}
          </CardContent>
        </Card>
      </div>
    </section>
  );
}
