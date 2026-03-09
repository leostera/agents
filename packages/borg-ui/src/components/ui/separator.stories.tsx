import type { Meta, StoryObj } from "@storybook/react-vite";

import { Separator } from "./separator";

const meta: Meta<typeof Separator> = {
  title: "UI/Separator",
  component: Separator,
};

export default meta;
type Story = StoryObj<typeof Separator>;

export const Horizontal: Story = {
  render: () => (
    <div style={{ width: "320px" }}>
      <p style={{ margin: "0 0 8px" }}>General settings</p>
      <Separator />
      <p style={{ margin: "8px 0 0" }}>Advanced settings</p>
    </div>
  ),
};

export const Vertical: Story = {
  render: () => (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: "10px",
        height: "40px",
      }}
    >
      <span>Runs</span>
      <Separator orientation="vertical" />
      <span>Actors</span>
      <Separator orientation="vertical" />
      <span>Tasks</span>
    </div>
  ),
};
