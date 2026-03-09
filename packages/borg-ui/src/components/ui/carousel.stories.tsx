import type { Meta, StoryObj } from "@storybook/react-vite";
import { useEffect, useState } from "react";

import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "./card";
import {
  Carousel,
  type CarouselApi,
  CarouselContent,
  CarouselItem,
  CarouselNext,
  CarouselPrevious,
} from "./carousel";

const slides = [
  {
    title: "Onboarding Completion",
    description: "78% this week",
  },
  {
    title: "Connected Providers",
    description: "OpenAI, Anthropic, Gemini",
  },
  {
    title: "Daily Active Actors",
    description: "1,284 actors",
  },
];

const meta: Meta<typeof Carousel> = {
  title: "UI/Carousel",
  component: Carousel,
};

export default meta;
type Story = StoryObj<typeof Carousel>;

export const DashboardCards: Story = {
  render: () => {
    const [api, setApi] = useState<CarouselApi>();
    const [current, setCurrent] = useState(1);
    const [count, setCount] = useState(slides.length);

    useEffect(() => {
      if (!api) return;

      setCount(api.scrollSnapList().length);
      setCurrent(api.selectedScrollSnap() + 1);

      const onSelect = () => {
        setCurrent(api.selectedScrollSnap() + 1);
      };

      api.on("select", onSelect);
      return () => {
        api.off("select", onSelect);
      };
    }, [api]);

    return (
      <div className="w-full max-w-xl">
        <Carousel setApi={setApi} opts={{ loop: true }}>
          <CarouselContent>
            {slides.map((slide) => (
              <CarouselItem key={slide.title}>
                <Card>
                  <CardHeader>
                    <CardTitle>{slide.title}</CardTitle>
                    <CardDescription>{slide.description}</CardDescription>
                  </CardHeader>
                  <CardContent className="text-muted-foreground text-sm">
                    Updated moments ago from live telemetry.
                  </CardContent>
                </Card>
              </CarouselItem>
            ))}
          </CarouselContent>
          <CarouselPrevious />
          <CarouselNext />
        </Carousel>
        <p className="text-muted-foreground mt-4 text-center text-xs">
          Slide {current} of {count}
        </p>
      </div>
    );
  },
};
