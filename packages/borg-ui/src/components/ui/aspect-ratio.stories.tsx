import type { Meta, StoryObj } from "@storybook/react";

import { AspectRatio } from "./aspect-ratio";

const meta: Meta<typeof AspectRatio> = {
  title: "UI/Aspect Ratio",
  component: AspectRatio,
  args: {
    ratio: 16 / 9,
  },
};

export default meta;
type Story = StoryObj<typeof AspectRatio>;

export const Default: Story = {
  render: (args) => (
    <div className="w-full max-w-xl overflow-hidden rounded-xl border">
      <AspectRatio {...args}>
        <img
          src="https://images.unsplash.com/photo-1485217988980-11786ced9454?auto=format&fit=crop&w=1200&q=80"
          alt="Workspace desk with laptop and notebook"
          className="size-full object-cover"
        />
      </AspectRatio>
    </div>
  ),
};

export const Portrait: Story = {
  args: {
    ratio: 3 / 4,
  },
  render: (args) => (
    <div className="w-64 overflow-hidden rounded-xl border">
      <AspectRatio {...args}>
        <img
          src="https://images.unsplash.com/photo-1517336714739-489689fd1ca8?auto=format&fit=crop&w=900&q=80"
          alt="Vertical product photo"
          className="size-full object-cover"
        />
      </AspectRatio>
    </div>
  ),
};
