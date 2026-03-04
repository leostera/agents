import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@borg/ui";
import React from "react";

type SectionProps = {
  children: React.ReactNode;
  className?: string;
};

type SectionToolbarProps = {
  children: React.ReactNode;
  className?: string;
};

type SectionEmptyProps = {
  icon: React.ComponentType<{ className?: string }>;
  title: string;
  description: string;
  action?: React.ReactNode;
  className?: string;
};

function mergeClasses(...values: Array<string | undefined | false>): string {
  return values.filter(Boolean).join(" ");
}

export function Section({ children, className }: SectionProps) {
  return (
    <section
      className={mergeClasses("flex h-full w-full min-h-0 flex-col", className)}
    >
      {children}
    </section>
  );
}

export function SectionToolbar({ children, className }: SectionToolbarProps) {
  return (
    <section
      className={mergeClasses("flex flex-wrap items-center gap-2", className)}
    >
      {children}
    </section>
  );
}

export function SectionContent({ children, className }: SectionProps) {
  return (
    <section className={mergeClasses("min-h-0 flex-1", className)}>
      {children}
    </section>
  );
}

export function SectionEmpty({
  icon: Icon,
  title,
  description,
  action,
  className,
}: SectionEmptyProps) {
  return (
    <Empty className={mergeClasses("h-full min-h-0 flex-1 border", className)}>
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <Icon />
        </EmptyMedia>
        <EmptyTitle>{title}</EmptyTitle>
        <EmptyDescription>{description}</EmptyDescription>
      </EmptyHeader>
      {action ? (
        <EmptyContent className="flex-row justify-center">
          {action}
        </EmptyContent>
      ) : null}
    </Empty>
  );
}
