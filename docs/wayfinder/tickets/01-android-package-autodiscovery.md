---
id: "01-android-package-autodiscovery"
title: "Android Package-to-Repository Auto-Discovery across Multi-Module Gradle and Manifest Trees"
label: "wayfinder:research"
type: "research"
mode: "AFK"
parent: "wayfinder-map"
blocked_by: []
assignee: "pedrogoncalves"
status: closed
resolution:
  date: "2026-10-15"
  findings: "../research/01-android-package-autodiscovery.md"
  gist: "5-pass static extractor (settings.gradle at root or application/, root ext/properties, module build.gradle with AGP 8+ namespace fallback & flavor overrides/suffixes, AndroidManifest.xml, and source package roots) backed by SQLite android_repositories and android_package_index tables with role-priority and crash_app_id_hint disambiguation."
---

# Android Package-to-Repository Auto-Discovery across Multi-Module Gradle and Manifest Trees

## Question

How do the 4 local Motorola Android PoC repositories (`AudioRecorder`, `FMRadio`, `MotoAssistPremium`, `motoDolbyUI`) declare their Android package identifiers (`applicationId`, `namespace`, `AndroidManifest.xml` `package`, build flavors/suffixes, and Java/Kotlin source package roots), and what deterministic parsing heuristic in `alexandria-daemon` (`discovery.rs`) is required so that any repository cloned into `~/projects/` is automatically indexed into a **Package-to-Repository Index** mapping crash `app_id` and stack-trace symbol prefixes to Gerrit project names, local Git paths, and `akashic/<app_id>/` Vault partitions?

## Resolution

Full research findings are documented in [01-android-package-autodiscovery.md](../research/01-android-package-autodiscovery.md).

### Key Findings Summary
1. **Nested Gradle Roots**: `AudioRecorder` and `motoDolbyUI` place `settings.gradle` under `application/settings.gradle`, whereas `FMRadio` and `MotoAssistPremium` place it at `<repo_root>/settings.gradle`.
2. **AGP 8+ Implicit `applicationId` Fallback**: `FMRadio/FMPlayer/build.gradle:107` declares `namespace = 'com.motorola.fmplayer'` with no explicit `applicationId`; the parser falls back to `namespace` for `com.android.application` modules.
3. **Product Flavor Overrides & Suffixes**: `motoDolbyUI` overrides `applicationId` in flavor `motoAudioEffects` (`com.motorola.motoaudioeffects`), and `FMRadio` appends `applicationIdSuffix ".prc"` (`com.motorola.fmplayer.prc`).
4. **Multiple Application Modules & Canonical `primary_app_id` Election**: Repositories contain companion services (`:recordingservice`), RRO overlays (`:overlay-revvl`), stubs (`:stubapp`), and test apps (`:testapp:app`, `:fmsdktestapp`). A deterministic module scoring heuristic elects the canonical `primary_app_id` for Vault partitioning:
   - `AudioRecorder` -> `akashic/com.motorola.audiorecorder`
   - `FMRadio` -> `akashic/com.motorola.fmplayer`
   - `MotoAssistPremium` -> `akashic/com.motorola.moto`
   - `motoDolbyUI` -> `akashic/com.motorola.dolby.dolbyui`
5. **Divergent Source Tree Packages**: Java/Kotlin source roots (`com.motorola.assist.*`, `com.motorola.showtips.*`, `com.motorola.airecorder.*`, `com.dimowner.audiorecorder.*`, `com.dolby.dax.*`, `com.motorola.android.fmradio.*`) are indexed from `src/main/{java,kotlin}` package declarations.
6. **Cross-Repo Shared Library Disambiguation**: Shared namespaces like `com.motorola.coreui` (present in both `AudioRecorder` and `motoDolbyUI`) are disambiguated using strict `role_priority` (`application_id` > `companion_application_id` > `flavor_application_id` > `overlay_application_id` > `module_namespace` > `manifest_package` > `source_prefix`) combined with `crash_app_id_hint` affinity in `resolve_stack_symbol`.
7. **SQLite Schema**: Defined `android_repositories` and `android_package_index` tables and resolution queries for `alexandria-core/src/store.rs`.
