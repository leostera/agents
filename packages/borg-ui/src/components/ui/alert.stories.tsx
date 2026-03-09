import { WarningCircleIcon } from "@phosphor-icons/react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { Alert, AlertAction, AlertDescription, AlertTitle } from "./alert";
import { Button } from "./button";

const meta: Meta<typeof Alert> = {
  title: "UI/Alert",
  component: Alert,
  args: {
    variant: "default",
  },
};

export default meta;
type Story = StoryObj<typeof Alert>;

export const Info: Story = {
  render: (args) => (
    <Alert {...args}>
      <WarningCircleIcon />
      <AlertTitle>Actor disconnected</AlertTitle>
      <AlertDescription>
        The HTTP port closed unexpectedly. Reconnect to continue receiving
        messages.
      </AlertDescription>
      <AlertAction>
        <Button size="sm">Reconnect</Button>
      </AlertAction>
    </Alert>
  ),
};

export const Destructive: Story = {
  args: {
    variant: "destructive",
  },
  render: (args) => (
    <Alert {...args}>
      <WarningCircleIcon />
      <AlertTitle>Invalid API key</AlertTitle>
      <AlertDescription>
        Authentication failed for provider <code>openai</code>. Rotate your key
        and try again.
      </AlertDescription>
      <AlertAction>
        <Button size="sm" variant="outline">
          Update key
        </Button>
      </AlertAction>
    </Alert>
  ),
};
