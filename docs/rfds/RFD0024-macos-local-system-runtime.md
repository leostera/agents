# RFD0024 - macOS Local System Runtime: VoiceWake, AppleScript, Shortcuts, and Apple Home

- Feature Name: `macos_local_system_runtime`
- Start Date: `2026-03-04`
- RFD PR: [leostera/borg#0000](https://github.com/leostera/borg/pull/0000)
- Borg Issue: [leostera/borg#0000](https://github.com/leostera/borg/issues/0000)

## Summary
[summary]: #summary

Add a first-class macOS local-system integration layer to Borg so agents can safely automate Apple-native workflows (AppleScript, Shortcuts, Notifications, app control), listen locally with VoiceWake-style wake flows, trigger Apple Home scenes, and optionally ship as a native menu bar app.  
The design keeps Borg’s existing model intact: Apps/Capabilities remain data, runtime tools remain typed, session-first ingress remains the core interaction path, and `borg-cli` remains the only binary crate.

## Motivation
[motivation]: #motivation

Borg already handles remote channels and typed runtime tools well, but it is still weak as a "local Mac assistant." Today, a user can chat through HTTP/Telegram/Discord, but they cannot reliably ask Borg to:

1. run Apple-native actions on the host machine with guardrails,
2. wake by voice in a local always-on mode,
3. orchestrate Apple Home scenes/devices from the same session model.

This gap blocks a major product direction: a local-first personal assistant that can act across the operating system where the user actually lives.

Concrete user outcomes this RFD enables:

1. "Hey Borg, turn off downstairs lights and start focus mode."
2. "Add this to Reminders and text my partner that I’m running late."
3. "Run my morning Mac setup shortcut and open project windows."
4. Hands-free voice interaction from one room away via wake phrase.
5. One-click status bar control for wake state, quick actions, and runtime health.

### Criteria fit

The spike is designed to satisfy these explicit criteria:

1. Voice wake:
   1. `voicewake` port runs always-on wake detection for `"Hey Borg"` then captures utterance audio into existing session ingress.
   2. menu bar UX shows clear state transitions while listening:
      1. waiting (`gray`),
      2. recording after wake phrase (`green`),
      3. error states (`red`) with actionable message.
   3. after wake phrase detection, capture continues until silence/final result; final utterance text excludes wake phrase prefix and is emitted as the "post-wake" buffer.
2. Common action notifications:
   1. local macOS banners via `MacOS-notifyStatus` and `MacOS-showNotification`.
3. iPhone-visible notifications:
   1. v0 relay path: run a user-owned shortcut from `MacOS-notifyStatus` (`iphone_shortcut_name`) to publish into iPhone-visible channels (for example iCloud Reminder/Message/other user automation).
   2. v1 path: dedicated iOS companion/APNs channel for stronger delivery guarantees and interactive actions.

## Guide-level explanation
[guide-level-explanation]: #guide-level-explanation

### Mental model

This proposal introduces two new local surfaces:

1. `macOS capability surface` (actions): typed runtime tools and App capabilities for AppleScript/Shortcuts/system automation.
2. `VoiceWake port` (ingress): local microphone-triggered turns that enter the same session pipeline as other ports.

Apple Home integrates in two steps:

1. Shortcuts-mediated control first (fast, low-friction).
2. Native HomeKit bridge later (deeper control, more setup).

### What contributors/operators should expect

1. On macOS, `borg start` can expose new macOS capabilities as a default app (`borg:app:macos-system`).
2. VoiceWake can run as an optional local port (`provider=voicewake`) and route audio turns into existing session/audio paths.
3. Apple Home actions are initially modeled as approved Shortcuts invocations, then optionally upgraded to native HomeKit capabilities.
4. Optional menu bar app provides UX shell (status, controls, permission hints) while Borg runtime logic stays in Rust.

### Example flow: voice to Apple Home scene

```mermaid
flowchart TD
  A[User says wake phrase] --> B[VoiceWake port captures utterance]
  B --> C[Port emits BorgInput Audio]
  C --> D[Session turn in borg-exec]
  D --> E[Agent chooses MacOS-runShortcut]
  E --> F[Shortcuts executes Home scene shortcut]
  F --> G[Assistant confirms completion]
```

### Example flow: AppleScript automation with guardrails

1. User asks to send a prewritten iMessage.
2. Agent calls `MacOS-runAppleScriptTemplate` with `template_id=messages.send`.
3. Runtime expands a vetted script template with typed parameters.
4. macOS prompts for Automation permission if required.
5. Result is returned as structured tool output and stored in normal tool call traces.

## Reference-level explanation
[reference-level-explanation]: #reference-level-explanation

### Scope

This RFD defines:

1. a macOS runtime/tool crate for typed local-system operations,
2. default App/Capability seeding for macOS operations,
3. VoiceWake-style local voice ingress as a first-class port,
4. Apple Home integration strategy (Shortcuts first, HomeKit second),
5. safety/permission model for local automation.

This RFD does not define:

1. a cross-platform Windows/Linux local-system parity layer,
2. SiriKit/App Intents direct registration from Borg in v0,
3. raw unrestricted host automation by default.

### 1. New crate: `crates/borg-macos`

Add a library crate that mirrors existing tool crates (`borg-codemode`, `borg-shellmode`) with:

1. `default_tool_specs() -> Vec<ToolSpec>`
2. `build_macos_toolchain(runtime: MacOsRuntime) -> Result<Toolchain<...>>`
3. typed request/response structs for each tool

`borg-cli` remains the only binary crate.

#### Initial tool set (v0)

1. `MacOS-listShortcuts`
2. `MacOS-runShortcut`
3. `MacOS-runAppleScriptTemplate`
4. `MacOS-runAppleScriptRaw` (disabled by policy default)
5. `MacOS-showNotification`
6. `MacOS-notifyStatus` (local + optional iPhone relay shortcut)
7. `MacOS-open` (URL/app/document)
8. `MacOS-say` (optional local spoken reply surface)

Implementation substrate in v0 is mostly command-based (`shortcuts`, `osascript`, `open`, `say`) wrapped in typed guards and policy checks, with native Rust crate support where it improves reliability (for example `mac-notification-sys` for local notifications, with AppleScript fallback).

### 2. Toolchain integration

`crates/borg-exec/src/tool_runner.rs` adds macOS tools into the default runtime toolchain on `target_os = "macos"`:

1. build code/shell/memory/fs/taskgraph/clockwork/admin/provider toolchains (current behavior),
2. merge `borg-macos` toolchain before returning final toolchain.

Non-macOS builds keep behavior unchanged.

### 3. Default app seeding

`crates/borg-apps/src/catalog.rs` adds:

1. `borg:app:macos-system`
2. capabilities generated from `borg-macos` tool specs
3. status `active` only on macOS (or active with runtime unsupported errors on other platforms)

This preserves the RFD0004 model: Apps/Capabilities are still data, and these operations are discoverable/grantable like any other capability.

### 4. VoiceWake as a local port

Add `voicewake` provider support in `borg-ports`:

1. `Provider::VoiceWake` in `port.rs`
2. `voicewake` module implementing `Port` trait
3. optional `PortContext::VoiceWake` with device metadata

#### VoiceWake behavior

1. Open local microphone stream.
2. Run wake detection loop:
   1. wake phrase mode (`"hey borg"` by default),
   2. push-to-talk mode,
   3. both mode.
3. On wake match, switch state to recording and track post-wake transcript buffer only.
4. Capture utterance window until silence/max duration.
5. Persist audio to BorgFS.
6. Emit `PortInput::Audio` to existing session turn pipeline.
7. Return assistant reply via notification and optional `say`.

The important architectural point: VoiceWake is ingress only. It does not create a parallel assistant runtime.

#### Suggested `settings_json` shape

```json
{
  "wake_mode": "phrase",
  "wake_phrase": "hey borg",
  "conversation_key": "voicewake:device:local-mac",
  "max_capture_ms": 12000,
  "silence_ms": 1000,
  "language_hint": "en-US",
  "speak_replies": true
}
```

### 5. Apple Home integration strategy

#### Phase A: Shortcuts-mediated Home control (default path)

Use `MacOS-runShortcut` for user-owned shortcuts that already include Home actions/scenes.

Examples:

1. `Home Good Night`
2. `Home Arrive`
3. `Home Movie Time`

This path avoids immediate HomeKit entitlement/signing complexity and works with user-customized scenes.

#### Phase B: deeper Home integration (post-v0 constraint-aware path)

Spike finding on this machine and toolchain (`Xcode 26.2`, Swift 6.2): `import HomeKit` is not available for macOS command-line/app targets (`no such module 'HomeKit'`), so a direct `HMHomeManager` bridge in a macOS helper is not currently a safe baseline assumption.

So the practical near-term path is:

1. keep Home control shortcut-mediated on macOS,
2. standardize shortcut naming/contracts for Home scenes and accessory intents,
3. treat direct HomeKit-style capabilities as a future path only if Apple exposes stable macOS SDK support again (or if we add a non-macOS companion service with explicit scope).

### 6. Distribution model: optional status bar app

This RFD supports two distribution modes on macOS:

1. CLI-only (current default): user runs `borg start`.
2. Menu bar app shell (new optional): app launches/monitors Borg and exposes lightweight controls.

Status bar app responsibilities:

1. Show runtime state (running, wake active, last command, errors).
2. Offer quick actions (`Start Borg`, `Stop Borg`, `Restart Runtime`, `Run Shortcut`, `Open Dashboard`).
3. Surface missing-permission diagnostics with direct remediation links.
4. Delegate all agent/runtime logic to Borg APIs; no duplicated orchestration logic in Swift.

Suggested implementation shape:

1. SwiftUI `MenuBarExtra` UI for modern menu bar UX.
2. `LSUIElement=true` so app can run as menu bar agent without Dock presence.
3. Optional `NSStatusItem` fallback for older AppKit-style behavior if needed.

### 7. Swift support and build workflow in this repo

The repository currently has no Swift/Xcode build lane. To ship a status bar app and optional Apple-framework bridge, add:

1. `apps/macos/BorgMenu/` (SwiftUI/AppKit menu bar app, Xcode project).
2. `apps/macos/BorgAppleBridge/` (optional Swift module/helper for deeper Apple APIs).
3. `scripts/macos/build_menu_app.sh` and `scripts/macos/dev_menu_app.sh`.

Proposed build contract:

1. Build Rust runtime first (`cargo build -p borg-cli` or release variant).
2. Build app via `xcodebuild` using either:
   1. an explicit `.xcodeproj` (`-project ... -scheme ...`), or
   2. a Swift package scheme directly from package directory (`xcodebuild -scheme <name> build`).
3. App embeds or locates `borg-cli` and starts it as managed background process.
4. UI talks to Borg over loopback API (`/health`, `/ports/http`, control endpoints).

Important build notes from spike:

1. `swift package generate-xcodeproj` is no longer available.
2. `xcodebuild -packagePath` is invalid in this Xcode; package builds should run from package dir with a scheme.

Release/signing requirements (for production app distribution):

1. App sandbox/entitlements configuration as needed for targeted capabilities.
2. Info.plist usage descriptions for microphone/speech/automation/home access where applicable.
3. Code signing + notarization + stapling pipeline in CI on macOS runners.
4. Hardened Runtime enabled for notarized distribution builds.

#### Startup/login behavior

To support "start with macOS" and operator service controls:

1. menu bar app should own Borg process lifecycle (`Start/Stop/Restart`) and stream subprocess logs into local diagnostics.
2. app should expose a direct "Open Login Items Settings" action so users can enable launch at login in System Settings.
3. for non-app installs, a LaunchAgent path remains a valid fallback for auto-start behavior.

### 7.1 Existing crate strategy (Rust-first, use proven deps)

To reduce bespoke glue code, the implementation should prefer maintained crates where they provide stable value:

1. Notifications:
   1. `mac-notification-sys` for native macOS notification delivery.
   2. Keep `osascript` fallback for resilience when native delivery fails.
2. Microphone capture / VoiceWake ingest:
   1. `cpal` for low-level cross-platform audio input stream handling.
   2. optional `ringbuf`/`crossbeam-channel` for lock-free buffering between capture and detection/transcription stages.
3. Wake-word detection:
   1. keep detector behind a trait (`WakeDetector`) so implementation can swap.
   2. start with a pluggable adapter approach rather than hard-binding to one vendor model in core runtime.
4. Speech-to-text for captured utterance:
   1. `whisper-rs` (or provider-backed STT via existing Borg provider path) as configurable backends.
   2. preserve provider-first routing in runtime config so local STT is optional, not mandatory.
5. Apple framework access from Rust:
   1. prefer command surfaces first (`shortcuts`, `osascript`, `open`, `say`) for low-friction coverage.
   2. when native framework access is needed, isolate it behind a Swift bridge helper/module instead of deep unsafe Objective-C bridging inside multiple Rust crates.

Selection criteria for any new dependency:

1. active maintenance and acceptable license,
2. small public API surface at Borg integration boundary,
3. deterministic failure/error reporting suitable for tool traces,
4. clear fallback path if crate behavior regresses.

### 7.2 iOS companion app spike (notification + control channel)

To strengthen iPhone-visible delivery beyond shortcut relay, add an iOS companion surface:

1. push channel:
   1. APNs payload carries normal `aps` alert plus a `borg` object with typed event metadata.
   2. iOS app decodes and stores Borg events locally, then deep-links into session views/actions.
2. control channel:
   1. iOS app can trigger bounded control actions (for example `Open on Mac`, `Acknowledge Error`) against Borg API.
   2. all actions remain policy-checked on Borg runtime side.
3. sync channel:
   1. app performs pull-sync for missed events after reconnect/background resume.
   2. cursor-based sync avoids duplicate render and supports delivery recovery.

Initial contract spiked in-repo (`apps/ios/BorgCompanionSpike`):

1. push `userInfo` shape:
   1. `aps` for user-visible notification rendering,
   2. `borg` typed payload (`version`, `eventId`, `sessionId`, `kind`, `title`, `body`, `createdAt`, `requiresOpen`).
2. pairing/sync models:
   1. `BorgCompanionPairingChallenge`,
   2. `BorgCompanionDeviceRegistration`,
   3. `BorgCompanionSyncRequest` / `BorgCompanionSyncResponse`.

### 8. Policy, permissions, and safety

Local automation is high impact. v0 must ship with strict defaults.

#### Runtime policy model

Store policy in macOS app connection settings (or dedicated policy table in follow-up):

1. `allowed_shortcuts`: allowlist names
2. `allowed_script_templates`: allowlist template IDs
3. `allow_raw_applescript`: default `false`
4. `max_execution_seconds`: bounded execution
5. `require_user_confirmation_for_high_risk`: default `true`

#### Permission boundaries

1. Apple Events automation prompts are expected for app control via AppleScript.
2. VoiceWake requires microphone/speech permissions where applicable.
3. Native HomeKit bridge requires HomeKit capability and usage descriptions.
4. Menu bar app packaging introduces signing/notarization correctness as an operational requirement.
5. AppleScript app-automation calls can hang pending automation/launch mediation; runtime must enforce hard timeouts.

Borg must fail with clear user-facing errors when permissions are missing, and include exact remediation steps.

### 9. Proposed macOS capability catalog (all-things-Apple direction)

First-wave capabilities (ship in this RFD scope):

1. Shortcuts run/list
2. AppleScript template execution
3. Notification/banner output
4. Status notifications with optional iPhone relay shortcut
5. Open app/URL/document
6. Optional spoken reply (`say`)

Second-wave capabilities (next RFD or extension):

1. Reminders create/list
2. Calendar event create/list
3. Messages send/read (where automation allows)
4. Mail draft/send template
5. Music playback control
6. Focus mode presets via Shortcuts
7. Clipboard read/write

Third-wave capabilities (deeper Apple Home + desktop control):

1. Native HomeKit scenes/accessories
2. Window/layout workspace profiles
3. Context-aware automations keyed by location/time/device state

### 10. Rollout plan

Phase 1: macOS tools substrate

1. Add `borg-macos` crate and tool specs.
2. Merge toolchain in `borg-exec`.
3. Seed `borg:app:macos-system`.
4. Ship allowlist/policy checks and trace logging.

Phase 2: status bar app shell + Swift build lane

1. Add `apps/macos/BorgMenu` project and local scripts.
2. Wire app to launch/monitor `borg-cli` and call loopback API.
3. Add CI job for `xcodebuild` on macOS runners (non-blocking at first, then required).

Phase 3: VoiceWake ingress

1. Add `voicewake` provider + port runtime.
2. Reuse existing audio persistence + transcription/session turn flow.
3. Add optional local notify/speak reply sink.

Phase 4: Apple Home depth

1. Productionize shortcut-mediated Home scenes.
2. Prototype richer Home scene/intent catalogs over Shortcuts.
3. Re-evaluate direct HomeKit path only if macOS SDK support is confirmed in target Xcode versions.

Phase 5: iOS companion app (APNs + sync)

1. ship `BorgCompanion` iOS app with APNs registration and Borg push payload decoding.
2. add pairing flow (short code + device registration) and secure token storage.
3. add cursor-based event sync endpoint in Borg API for missed notifications.
4. add interactive notification actions mapped to safe Borg control endpoints.

### 11. Acceptance criteria

1. On macOS, agent can invoke `MacOS-runShortcut` and `MacOS-runAppleScriptTemplate` through normal tool/capability paths.
2. VoiceWake can wake on `"Hey Borg"`, capture utterance audio, and produce a session reply without creating ad-hoc task/session logic.
3. Common runtime events (`response available`, `error`) can emit local notifications via `MacOS-notifyStatus`.
4. iPhone-visible notification relay works through operator-configured shortcut channels.
5. iOS companion/APNs channel can receive and decode typed Borg notification events.
6. Apple Home scene control works through Shortcuts with explicit policy allowlists.
7. Missing permission states return actionable remediation messages.
8. Optional menu bar app can start/stop/observe Borg runtime without duplicating agent logic.

### 12. Experimental findings (2026-03-04 spike)

The following findings come from live local experiments in this branch/worktree:

1. `shortcuts` CLI:
   1. `shortcuts list --show-identifiers` works and returns stable UUID identifiers.
   2. `shortcuts run` can fail with actionable policy errors, including:
      1. "scripting actions not allowed" (Shortcuts security setting),
      2. action-specific dependency errors (for example missing required app/service).
2. AppleScript (`osascript`):
   1. local scripts (`return`, `do shell script`, `display notification`) work.
   2. cross-app Apple Events (`tell application "Finder"/"Music"/"System Events"`) can block without timeout.
   3. wrapping with `with timeout of N seconds` reliably returns `-1712` instead of hanging indefinitely.
3. Swift/tooling:
   1. Swift 6.2 + Xcode 26.2 are available locally.
   2. `xcodebuild -scheme <package>` from Swift package directory works.
   3. `swift package generate-xcodeproj` is unavailable; older guidance should not be used.
4. VoiceWake substrate feasibility:
   1. Rust `cpal` spike can enumerate and stream from default microphone (`MacBook Pro Microphone`) with sample capture.
   2. Swift Speech menu bar spike validates always-on phrase-gated flow with explicit waiting/recording/error state transitions.
   3. wake phrase parsing now logs:
      1. `wake_prefix` (text before first wake),
      2. `wake_between` chunks (text between repeated wake phrases),
      3. `wake_buffer` (post-last-wake transcript buffer used as utterance).
   4. recognizer restart hardening is required in practice:
      1. suppress idle "No speech detected" loops by fast restart without red error transitions,
      2. guard callbacks by recognition session id to ignore stale asynchronous results after restart.
   5. This validates a native local mic-ingress implementation path for `voicewake` port while preserving room for a dedicated wake-word model later.
5. HomeKit constraint:
   1. Swift `import HomeKit` fails on this macOS target (`no such module 'HomeKit'`), so direct HomeKit-on-macOS is not assumed in v0/v1.
6. Borg local build/runtime prerequisites observed during spike:
   1. `cargo run -p borg-cli` requires `packages/borg-agent-sdk/dist/borg-agent-sdk.min.js`.
   2. `borg-api` build requires dashboard dist at `packages/borg-app/dist`.
   3. using shared `~/.borg` may fail with migration drift between branches; isolated homes/dirs should be used for repeatable spikes.
7. Spike artifacts implemented in this branch:
   1. Rust macOS tool crate scaffold: `crates/borg-macos` with tested command wrappers and policy gates.
   2. Runtime integration: macOS toolchain merge in `crates/borg-exec/src/tool_runner.rs`.
   3. Default app seeding: `borg:app:macos-system` in `crates/borg-apps/src/catalog.rs`.
   4. Native notification path spike via `mac-notification-sys` in `borg-macos` (`show_notification` uses native send first, then falls back to AppleScript).
   5. Swift menu bar + voicewake scaffold: `apps/macos/BorgMenuSpike` plus scripts under `scripts/macos/`.
   6. menu bar service controls (`Start Borg`, `Stop Borg`, `Restart Borg`) with subprocess lifecycle handling and streamed stdout/stderr logs.
   7. startup UX hook via login-items settings action in menu bar app.
8. iOS companion package spike: `apps/ios/BorgCompanionSpike` with typed push decoder + pairing/sync contracts + fixture decoder executable.
9. iOS companion build findings:
   1. `swift build`/`swift test` pass for companion package.
   2. `xcodebuild -scheme BorgCompanionCore -destination 'generic/platform=iOS Simulator' build` fails on this host due unavailable iOS simulator destination and missing installed iOS platform/runtime components.
   3. `xcrun simctl list devices` returns no simulator devices in this environment.
   4. `simctl push` workflow script added (`scripts/ios/spike_companion_push_to_sim.sh`) and currently reports no booted simulator on this host, which is expected until simulator runtime/device provisioning is completed.

### 12.1 Spike runbook (current branch)

Voicewake + menu bar spike:

```bash
scripts/macos/spike_menu_build.sh
BORG_CLI_PATH=/abs/path/to/borg-cli \
BORG_CLI_ARGS="start" \
BORG_VOICEWAKE_PHRASE="hey borg" \
scripts/macos/spike_voicewake_run.sh
```

iOS companion spike:

```bash
scripts/ios/spike_companion_build.sh
scripts/ios/spike_companion_test.sh
scripts/ios/spike_companion_decode_fixture.sh
```

## Drawbacks
[drawbacks]: #drawbacks

1. Local automation increases blast radius if policy is misconfigured.
2. Apple platform permissions are user-visible and can cause onboarding friction.
3. VoiceWake adds always-on resource usage (CPU/mic/battery).
4. macOS Home integration depth is constrained by available public SDK surface and may remain shortcut-mediated for longer than desired.
5. Adding a Swift/Xcode lane increases build/release complexity for contributors and CI.

## Rationale and alternatives
[rationale-and-alternatives]: #rationale-and-alternatives

### Why this design

1. It aligns with existing Borg architecture: session-first ingress + typed toolchains + data-driven capabilities.
2. It ships value early with Shortcuts/AppleScript while preserving a clean path to deeper Apple-native APIs.
3. It avoids introducing a second independent assistant runtime.

### Alternatives considered

1. Shell-only approach (`ShellMode-executeCommand` for everything):
   1. rejected because it lacks stable typed contracts and policy hooks.
2. Native HomeKit-only from day one:
   1. rejected due entitlement/signing complexity and slower time-to-value.
3. Voice assistant as an external standalone app:
   1. rejected as primary direction because it fragments session/task/memory semantics outside Borg.
4. Replace `borg-cli` with a Swift app as the primary runtime:
   1. rejected to preserve existing Rust runtime architecture and avoid a full platform rewrite.

## Prior art
[prior-art]: #prior-art

1. OpenClaw VoiceWake mode demonstrates practical wake-phrase + push-to-talk local voice patterns and is a useful UX reference for hands-free operation.
2. Apple’s Shortcuts CLI on macOS provides a native command-line bridge for running user automations from local tools.
3. AppleScript/Automation permission model on macOS defines clear user-consent boundaries for app-to-app control.
4. Apple Home/HomeKit documentation shows capability and permission requirements for direct accessory control.

## Unresolved questions
[unresolved-questions]: #unresolved-questions

1. Should raw AppleScript ever be enabled for autonomous agents, or only for interactive operator-approved sessions?
2. Do we keep VoiceWake transcription provider-backed in v1, or require embedded local STT for privacy/offline-first behavior?
3. Given current macOS SDK constraints around HomeKit, should deeper Home control rely on Shortcuts only, or use a non-macOS companion bridge?
4. What is the minimal safe default allowlist for first-run macOS capability onboarding?
5. Do we standardize on Xcode project files, SwiftPM packages, or both for maintainable Swift builds in-repo?
6. Should APNs delivery be direct from Borg backend, via relay provider, or via the macOS host as sender-of-record?
7. Is transcript-driven wake gating sufficient for v1, or do we require a dedicated wake-word detector to reduce false positives and idle CPU/battery cost?

## Future possibilities
[future-possibilities]: #future-possibilities

1. App Intents bridge so Borg can invoke user-exposed app actions directly (beyond generic Shortcuts names).
2. Cross-device Apple workflows (Mac + iPhone + Watch) using shared shortcuts and handoff contexts.
3. Local "ambient assistant" mode combining VoiceWake, Clockwork schedules, and Home state signals.
4. Operator policy UI in dashboard for per-capability trust levels, required confirmations, and audit playback.
5. Expanded multimodal loop: voice input + on-screen context + typed summary memory per session.
6. If Apple exposes stable macOS Home SDK support, revisit direct typed Home entities/capabilities in Borg.

## Research references

1. OpenClaw VoiceWake docs: https://docs.openclaw.ai/voice-assistant/voicewake-mode
2. Shortcuts command-line usage on Mac: https://support.apple.com/guide/shortcuts-mac/run-shortcuts-from-the-command-line-apd455c82f02/mac
3. macOS automation permissions (Apple Events): https://support.apple.com/guide/mac-help/allow-apps-to-automate-and-control-other-apps-mchl108e1718/mac
4. Recognizing speech in live audio (Speech framework): https://developer.apple.com/documentation/speech/recognizing-speech-in-live-audio
5. HomeKit sample setup and entitlement notes: https://developer.apple.com/documentation/homekit/configuring-a-home-automation-device
6. SwiftUI menu bar APIs (`MenuBarExtra`): https://developer.apple.com/documentation/swiftui/menubarextra
7. `NSStatusItem` reference: https://developer.apple.com/documentation/appkit/nsstatusitem
8. Building with `xcodebuild` (TN2339): https://developer.apple.com/library/archive/technotes/tn2339/_index.html
9. Notarizing macOS software: https://developer.apple.com/documentation/security/notarizing_macos_software_before_distribution
10. APNs overview: https://developer.apple.com/documentation/usernotifications/setting_up_a_remote_notification_server
11. Requesting notification authorization: https://developer.apple.com/documentation/usernotifications/unusernotificationcenter/requestauthorization(options:completionhandler:)
