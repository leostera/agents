"use client";

import { Check, Link as LinkIcon } from "lucide-react";
import * as React from "react";
import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

type EntityLinkProps = {
  uri: string;
  name: string;
  className?: string;
};

export function EntityLink({ uri, name, className }: EntityLinkProps) {
  const [copied, setCopied] = React.useState(false);

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(uri);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    } catch {
      setCopied(false);
    }
  };

  return (
    <div className={className ?? "inline-flex items-center gap-1"}>
      <span>{name}</span>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            onClick={(event) => {
              event.preventDefault();
              event.stopPropagation();
              void handleCopy();
            }}
            title={copied ? "Copied" : "Copy URI"}
            aria-label={copied ? "Copied" : "Copy URI"}
          >
            {copied ? (
              <Check className="size-3.5" />
            ) : (
              <LinkIcon className="size-3.5" />
            )}
          </Button>
        </TooltipTrigger>
        <TooltipContent>{copied ? "Copied" : "Copy URI"}</TooltipContent>
      </Tooltip>
    </div>
  );
}
