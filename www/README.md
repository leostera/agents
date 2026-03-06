# WWW Workspace

This directory contains all OpenBorg web/doc surfaces.

## Projects

- `dev.openborg`: marketing site + platform developer docs.
- `dev.openborg.docs`: standalone Borg operator documentation site.
- `dev.openborg.build`: reserved folder for build/deploy artifact wiring.

## Root Scripts

From repo root:

- `bun run dev:www` -> runs `www/dev.openborg`
- `bun run dev:www-standalone` -> runs `www/dev.openborg.docs`
- `bun run build:www` -> builds `www/dev.openborg`
- `bun run build:www-standalone` -> builds `www/dev.openborg.docs`
- `bun run dev` -> runs Vite + both Astro servers + Storybook together

## Default Local Ports

- `bun run dev:www` -> `http://localhost:4321`
- `bun run dev:www-standalone` -> `http://localhost:4322`
- `bun run storybook` -> `http://localhost:6006`
