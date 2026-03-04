# BorgMenuSpike

Swift menu bar + voicewake spike used to validate:

1. menu bar distribution shape,
2. wake phrase detection (`"hey borg"` by default),
3. recording indication in status bar (green bubble),
4. final transcription after silence,
5. Borg daemon controls (`Start Borg`, `Stop Borg`, `Restart Borg`).

## Build

```bash
scripts/macos/spike_menu_build.sh
```

## Run

```bash
scripts/macos/spike_menu_run.sh
```

## Voicewake-only Shortcut

```bash
scripts/macos/spike_voicewake_run.sh
```

Environment variables:

1. `BORG_VOICEWAKE_PHRASE` default: `hey borg`
2. `BORG_VOICEWAKE_SILENCE_SECONDS` default: `1.4`
3. `BORG_MENU_SPIKE_AUTOTERMINATE_SECONDS` default: `0` (no auto-exit)
4. `BORG_MENU_SPIKE_AUTO_START_BORG` default: `false`
5. `BORG_CLI_PATH` optional path to `borg`/`borg-cli` executable
6. `BORG_CLI_ARGS` optional args (default: `start`)
7. `BORG_VOICEWAKE_DEBUG_TRANSCRIPTS` default: `false` (prints live recognized transcript chunks)
