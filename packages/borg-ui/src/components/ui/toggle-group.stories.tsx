import {
  AlignCenterHorizontalIcon,
  AlignLeftIcon,
  AlignRightIcon,
} from "@phosphor-icons/react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";

import { ToggleGroup, ToggleGroupItem } from "./toggle-group";

const meta: Meta<typeof ToggleGroup> = {
  title: "UI/ToggleGroup",
  component: ToggleGroup,
};

export default meta;
type Story = StoryObj<typeof ToggleGroup>;

export const Single: Story = {
  render: () => {
    const [value, setValue] = useState("left");

    return (
      <ToggleGroup
        type="single"
        value={value}
        onValueChange={(next) => next && setValue(next)}
        variant="outline"
      >
        <ToggleGroupItem value="left" aria-label="Align left">
          <AlignLeftIcon />
        </ToggleGroupItem>
        <ToggleGroupItem value="center" aria-label="Align center">
          <AlignCenterHorizontalIcon />
        </ToggleGroupItem>
        <ToggleGroupItem value="right" aria-label="Align right">
          <AlignRightIcon />
        </ToggleGroupItem>
      </ToggleGroup>
    );
  },
};

export const Multiple: Story = {
  render: () => {
    const [value, setValue] = useState<string[]>(["logs", "metrics"]);

    return (
      <ToggleGroup
        type="multiple"
        value={value}
        onValueChange={setValue}
        spacing={1}
      >
        <ToggleGroupItem value="logs">Logs</ToggleGroupItem>
        <ToggleGroupItem value="metrics">Metrics</ToggleGroupItem>
        <ToggleGroupItem value="events">Events</ToggleGroupItem>
      </ToggleGroup>
    );
  },
};
