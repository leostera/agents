import type { Meta, StoryObj } from "@storybook/react";

import { Textarea } from "./textarea";

const meta: Meta<typeof Textarea> = {
  title: "UI/Textarea",
  component: Textarea,
  args: {
    placeholder: "Describe what happened...",
    rows: 4,
  },
};

export default meta;
type Story = StoryObj<typeof Textarea>;

export const Default: Story = {};

export const WithValue: Story = {
  args: {
    defaultValue:
      "When I connect a new provider, I expect the session summary to appear in the sidebar immediately.",
  },
};

export const Invalid: Story = {
  args: {
    "aria-invalid": true,
    defaultValue: "Missing required details.",
  },
};
