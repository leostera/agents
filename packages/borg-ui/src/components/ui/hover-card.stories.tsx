import type { Meta, StoryObj } from "@storybook/react-vite";

import { Avatar, AvatarFallback, AvatarImage } from "./avatar";
import { HoverCard, HoverCardContent, HoverCardTrigger } from "./hover-card";

const meta: Meta<typeof HoverCard> = {
  title: "UI/Hover Card",
  component: HoverCard,
};

export default meta;
type Story = StoryObj<typeof HoverCard>;

export const Default: Story = {
  render: () => (
    <HoverCard>
      <HoverCardTrigger asChild>
        <a href="#" style={{ fontSize: "12px" }}>
          @leostera
        </a>
      </HoverCardTrigger>
      <HoverCardContent>
        <div style={{ display: "flex", gap: "10px" }}>
          <Avatar>
            <AvatarImage src="https://i.pravatar.cc/80?img=52" alt="Leo S." />
            <AvatarFallback>LS</AvatarFallback>
          </Avatar>
          <div>
            <p style={{ margin: 0, fontWeight: 500 }}>Leo S.</p>
            <p style={{ margin: "2px 0 0", color: "var(--muted-foreground)" }}>
              Maintains runtime contracts and release workflow.
            </p>
          </div>
        </div>
      </HoverCardContent>
    </HoverCard>
  ),
};
