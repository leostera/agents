import type { Meta, StoryObj } from "@storybook/react-vite";

import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "./card";

const meta: Meta<typeof Card> = {
  title: "UI/Card",
  component: Card,
};

export default meta;
type Story = StoryObj<typeof Card>;

export const Default: Story = {
  render: () => (
    <Card>
      <CardHeader>
        <CardTitle>Users</CardTitle>
        <CardDescription>Registered accounts in the workspace</CardDescription>
      </CardHeader>
      <CardContent>
        <p style={{ margin: 0 }}>1,248 users</p>
      </CardContent>
    </Card>
  ),
};

export const WithLegacyTitleProp: Story = {
  render: () => (
    <Card title="Sessions">
      <CardContent>
        <p style={{ margin: 0 }}>42 active sessions</p>
      </CardContent>
    </Card>
  ),
};
