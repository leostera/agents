# BorgCompanionSpike

iOS companion spike for Borg notification delivery, pairing contracts, and sync protocol modeling.

## Scope

1. Define push payload contract (`aps` + `borg`) for iPhone-visible notifications.
2. Define pairing/sync request-response models for a future iOS app.
3. Verify local Swift build/test flow in-repo and iOS simulator compile path.

## Build and test

```bash
scripts/ios/spike_companion_build.sh
scripts/ios/spike_companion_test.sh
```

## Decode a sample push payload

```bash
scripts/ios/spike_companion_decode_fixture.sh
```

## Push fixture into booted simulator

```bash
scripts/ios/spike_companion_push_to_sim.sh
```

Optional environment variables:

1. `BORG_COMPANION_BUNDLE_ID` (default: `com.borg.companion.spike`)
2. `BORG_SIMULATOR_UDID` (if you want to target a specific simulator)

## iOS compile check

This validates package targets can compile against iOS Simulator SDK:

```bash
scripts/ios/spike_companion_ios_build.sh
```

If this fails with destination/platform errors, install iOS platform components in Xcode settings and add at least one simulator runtime/device.

## Notes

1. This spike is package-based; it is not a full `.xcodeproj` app yet.
2. APNs entitlements, provisioning, and App Store distribution are out of scope for this spike.
3. Current host finding: `xcodebuild` iOS Simulator destination is unavailable until extra platform/runtime components are installed.
