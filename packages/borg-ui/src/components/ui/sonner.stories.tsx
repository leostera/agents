import type { Meta, StoryObj } from "@storybook/react-vite";
import { ThemeProvider } from "next-themes";
import { toast } from "sonner";

import { Button } from "./button";
import { Toaster } from "./sonner";

const meta: Meta<typeof Toaster> = {
  title: "UI/Sonner",
  component: Toaster,
  decorators: [
    (Story) => (
      <ThemeProvider attribute="class" forcedTheme="light">
        <div className="min-h-[240px] p-2">
          <Story />
        </div>
      </ThemeProvider>
    ),
  ],
};

export default meta;
type Story = StoryObj<typeof Toaster>;

export const ToastStates: Story = {
  render: () => (
    <>
      <div className="flex flex-wrap gap-2">
        <Button size="sm" onClick={() => toast.success("Provider connected")}>
          Success
        </Button>
        <Button
          size="sm"
          variant="outline"
          onClick={() => toast.info("Syncing actor snapshots")}
        >
          Info
        </Button>
        <Button
          size="sm"
          variant="secondary"
          onClick={() => toast.warning("API key expires in 3 days")}
        >
          Warning
        </Button>
        <Button
          size="sm"
          variant="destructive"
          onClick={() => toast.error("Connection failed")}
        >
          Error
        </Button>
      </div>
      <Toaster richColors closeButton />
    </>
  ),
};

export const PromiseLifecycle: Story = {
  render: () => (
    <>
      <Button
        onClick={() =>
          toast.promise(new Promise((resolve) => setTimeout(resolve, 1500)), {
            loading: "Validating API key...",
            success: "Provider is ready",
            error: "Validation failed",
          })
        }
      >
        Simulate provider connect
      </Button>
      <Toaster richColors />
    </>
  ),
};
