import type { Meta, StoryObj } from "@storybook/react";

import { Button } from "./button";
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
  SheetTrigger,
} from "./sheet";

const meta: Meta<typeof Sheet> = {
  title: "UI/Sheet",
  component: Sheet,
};

export default meta;
type Story = StoryObj<typeof Sheet>;

export const RightInspector: Story = {
  render: () => (
    <Sheet>
      <SheetTrigger asChild>
        <Button variant="outline">Open inspector</Button>
      </SheetTrigger>
      <SheetContent>
        <SheetHeader>
          <SheetTitle>Session inspector</SheetTitle>
          <SheetDescription>
            Track token usage, model route, and active tool calls.
          </SheetDescription>
        </SheetHeader>
        <div className="px-6 pb-3 text-xs/relaxed space-y-1.5">
          <p>Model: gpt-4.1-mini</p>
          <p>Input tokens: 1,328</p>
          <p>Output tokens: 412</p>
        </div>
        <SheetFooter>
          <Button>Save defaults</Button>
        </SheetFooter>
      </SheetContent>
    </Sheet>
  ),
};

export const BottomActions: Story = {
  render: () => (
    <Sheet>
      <SheetTrigger asChild>
        <Button variant="secondary">Bulk actions</Button>
      </SheetTrigger>
      <SheetContent side="bottom" className="max-w-2xl mx-auto rounded-t-xl">
        <SheetHeader>
          <SheetTitle>Apply action to 14 sessions</SheetTitle>
          <SheetDescription>
            Choose one operation and apply it to selected sessions.
          </SheetDescription>
        </SheetHeader>
        <SheetFooter className="sm:flex-row">
          <Button variant="outline">Archive</Button>
          <Button variant="destructive">Delete</Button>
        </SheetFooter>
      </SheetContent>
    </Sheet>
  ),
};
