import type { Meta, StoryObj } from "@storybook/react";
import { useEffect, useState } from "react";

import { Progress } from "./progress";

const meta: Meta<typeof Progress> = {
  title: "UI/Progress",
  component: Progress,
  args: {
    value: 66,
  },
};

export default meta;
type Story = StoryObj<typeof Progress>;

export const Default: Story = {};

export const Loading: Story = {
  render: () => {
    const [value, setValue] = useState(12);

    useEffect(() => {
      const id = window.setInterval(() => {
        setValue((current) => (current >= 100 ? 100 : current + 8));
      }, 400);

      return () => window.clearInterval(id);
    }, []);

    return <Progress value={value} />;
  },
};
