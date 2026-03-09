import type { Meta, StoryObj } from "@storybook/react-vite";

import { Button } from "./button";
import {
  Drawer,
  DrawerClose,
  DrawerContent,
  DrawerDescription,
  DrawerFooter,
  DrawerHeader,
  DrawerTitle,
  DrawerTrigger,
} from "./drawer";

const meta: Meta<typeof Drawer> = {
  title: "UI/Drawer",
  component: Drawer,
};

export default meta;
type Story = StoryObj<typeof Drawer>;

export const BottomCheckout: Story = {
  render: () => (
    <Drawer>
      <DrawerTrigger asChild>
        <Button variant="outline">Open checkout</Button>
      </DrawerTrigger>
      <DrawerContent>
        <DrawerHeader>
          <DrawerTitle>Upgrade to Pro</DrawerTitle>
          <DrawerDescription>
            Enable long-running actors, logs retention, and team seats.
          </DrawerDescription>
        </DrawerHeader>
        <div className="px-4 pb-2 text-xs/relaxed">
          <div className="border rounded-md p-3 bg-muted/20">
            <p className="font-medium">Pro plan</p>
            <p className="text-muted-foreground">$29/month billed monthly</p>
          </div>
        </div>
        <DrawerFooter>
          <Button>Continue</Button>
          <DrawerClose asChild>
            <Button variant="outline">Not now</Button>
          </DrawerClose>
        </DrawerFooter>
      </DrawerContent>
    </Drawer>
  ),
};

export const RightPanel: Story = {
  render: () => (
    <Drawer direction="right">
      <DrawerTrigger asChild>
        <Button variant="outline">Actor details</Button>
      </DrawerTrigger>
      <DrawerContent>
        <DrawerHeader>
          <DrawerTitle>actor_2db4</DrawerTitle>
          <DrawerDescription>
            Started 6 minutes ago from the HTTP ingress port.
          </DrawerDescription>
        </DrawerHeader>
        <div className="px-4 pb-2 text-xs/relaxed text-muted-foreground">
          Active model: gpt-4.1-mini
        </div>
        <DrawerFooter>
          <DrawerClose asChild>
            <Button variant="outline">Close</Button>
          </DrawerClose>
        </DrawerFooter>
      </DrawerContent>
    </Drawer>
  ),
};
