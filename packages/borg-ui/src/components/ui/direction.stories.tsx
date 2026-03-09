import type { Meta, StoryObj } from "@storybook/react-vite";

import { Badge } from "./badge";
import { DirectionProvider, useDirection } from "./direction";

function DirectionPreview() {
  const dir = useDirection();

  return (
    <div className="border rounded-lg p-4 w-full max-w-md" dir={dir}>
      <div className="flex items-center justify-between mb-3">
        <span className="text-xs text-muted-foreground">Current direction</span>
        <Badge variant="outline">{dir.toUpperCase()}</Badge>
      </div>
      <div className="bg-muted/50 rounded-md p-3 text-xs/relaxed">
        <p className="font-medium">Composer alignment preview</p>
        <p className="text-muted-foreground">
          The avatar and text flow follow the provider direction context.
        </p>
      </div>
    </div>
  );
}

const meta: Meta<typeof DirectionProvider> = {
  title: "UI/Direction",
  component: DirectionProvider,
};

export default meta;
type Story = StoryObj<typeof DirectionProvider>;

export const LeftToRight: Story = {
  render: () => (
    <DirectionProvider direction="ltr">
      <DirectionPreview />
    </DirectionProvider>
  ),
};

export const RightToLeft: Story = {
  render: () => (
    <DirectionProvider direction="rtl">
      <DirectionPreview />
    </DirectionProvider>
  ),
};
