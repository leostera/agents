import type { Meta, StoryObj } from "@storybook/react";
import { useState } from "react";

import { Tabs, TabsContent, TabsList, TabsTrigger } from "./tabs";

const meta: Meta<typeof Tabs> = {
  title: "UI/Tabs",
  component: Tabs,
  args: {
    defaultValue: "overview",
  },
};

export default meta;
type Story = StoryObj<typeof Tabs>;

export const Default: Story = {
  render: () => {
    const [value, setValue] = useState("overview");

    return (
      <Tabs value={value} onValueChange={setValue} style={{ width: 500 }}>
        <TabsList>
          <TabsTrigger value="overview">Overview</TabsTrigger>
          <TabsTrigger value="ports">Ports</TabsTrigger>
          <TabsTrigger value="history">History</TabsTrigger>
        </TabsList>
        <TabsContent value="overview">
          Run status, connection health, and recent completions.
        </TabsContent>
        <TabsContent value="ports">
          3 active ports: `telegram`, `http`, and `cli`.
        </TabsContent>
        <TabsContent value="history">
          No regressions in the last 7 days.
        </TabsContent>
      </Tabs>
    );
  },
};

export const LineVariant: Story = {
  render: () => (
    <Tabs defaultValue="all" style={{ width: 420 }}>
      <TabsList variant="line">
        <TabsTrigger value="all">All</TabsTrigger>
        <TabsTrigger value="open">Open</TabsTrigger>
        <TabsTrigger value="closed">Closed</TabsTrigger>
      </TabsList>
      <TabsContent value="all">14 actors total.</TabsContent>
      <TabsContent value="open">5 actors currently active.</TabsContent>
      <TabsContent value="closed">9 actors completed.</TabsContent>
    </Tabs>
  ),
};
