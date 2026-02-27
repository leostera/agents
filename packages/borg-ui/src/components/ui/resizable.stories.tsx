import type { Meta, StoryObj } from "@storybook/react";

import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "./resizable";

const meta: Meta<typeof ResizablePanelGroup> = {
  title: "UI/Resizable",
  component: ResizablePanelGroup,
};

export default meta;
type Story = StoryObj<typeof ResizablePanelGroup>;

export const HorizontalPanels: Story = {
  render: () => (
    <div className="h-64 border rounded-lg overflow-hidden">
      <ResizablePanelGroup direction="horizontal">
        <ResizablePanel defaultSize={35} minSize={20}>
          <div className="h-full p-3 text-xs/relaxed bg-muted/20">
            Session timeline
          </div>
        </ResizablePanel>
        <ResizableHandle withHandle />
        <ResizablePanel defaultSize={65}>
          <div className="h-full p-3 text-xs/relaxed">
            Conversation transcript and tool outputs.
          </div>
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  ),
};

export const VerticalPanels: Story = {
  render: () => (
    <div className="h-72 border rounded-lg overflow-hidden">
      <ResizablePanelGroup direction="vertical">
        <ResizablePanel defaultSize={55}>
          <div className="h-full p-3 text-xs/relaxed">Request payload</div>
        </ResizablePanel>
        <ResizableHandle />
        <ResizablePanel defaultSize={45}>
          <div className="h-full p-3 text-xs/relaxed bg-muted/20">
            Response preview
          </div>
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  ),
};
