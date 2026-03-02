import { Badge } from "@borg/ui";
import React from "react";

export function BehaviorsPage() {
  return (
    <section className="space-y-3">
      <div className="flex items-center gap-2">
        <h2 className="text-base font-semibold">Behaviors</h2>
        <Badge variant="outline">Draft</Badge>
      </div>
      <p className="text-muted-foreground text-sm">
        Behaviors replace policy concerns previously carried by agents.
        CRUD and capability composition will be implemented next.
      </p>
    </section>
  );
}
