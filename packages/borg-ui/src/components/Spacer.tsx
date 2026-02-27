import React from "react";

type SpacerProps = {
  size?: number;
};

const DEFAULT_SPACER_SIZE = 10;

export function Spacer(props: SpacerProps) {
  return (
    <div
      style={{ height: props.size ?? DEFAULT_SPACER_SIZE }}
      aria-hidden="true"
    />
  );
}
