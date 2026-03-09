import type { Meta, StoryObj } from "@storybook/react-vite";

import { Badge } from "./badge";

const meta: Meta<typeof Badge> = {
  title: "UI/Badge",
  component: Badge,
  args: {
    children: "Active",
    variant: "default",
  },
};

export default meta;
type Story = StoryObj<typeof Badge>;

export const Default: Story = {};

export const Variants: Story = {
  render: () => (
    <div style={{ display: "flex", gap: "8px", flexWrap: "wrap" }}>
      <Badge>Default</Badge>
      <Badge variant="secondary">Secondary</Badge>
      <Badge variant="destructive">Error</Badge>
      <Badge variant="outline">Queued</Badge>
      <Badge variant="ghost">Draft</Badge>
      <Badge variant="link">Open logs</Badge>
    </div>
  ),
};
