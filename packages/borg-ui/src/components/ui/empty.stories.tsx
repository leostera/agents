import { ArchiveBoxIcon } from "@phosphor-icons/react";
import type { Meta, StoryObj } from "@storybook/react";

import { Button } from "./button";
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "./empty";

const meta: Meta<typeof Empty> = {
  title: "UI/Empty",
  component: Empty,
};

export default meta;
type Story = StoryObj<typeof Empty>;

export const Default: Story = {
  render: () => (
    <Empty style={{ maxWidth: 520 }}>
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <ArchiveBoxIcon />
        </EmptyMedia>
        <EmptyTitle>No sessions yet</EmptyTitle>
        <EmptyDescription>
          Start your first conversation to create a session and capture context.
        </EmptyDescription>
      </EmptyHeader>
      <EmptyContent>
        <Button size="sm">Start session</Button>
      </EmptyContent>
    </Empty>
  ),
};
