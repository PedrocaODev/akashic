# Research Report: Android Package-to-Repository Auto-Discovery across Multi-Module Gradle and Manifest Trees

**Ticket**: [Android Package-to-Repository Auto-Discovery across Multi-Module Gradle and Manifest Trees](../tickets/01-android-package-autodiscovery.md)  
**Date**: 2026-10-15  
**Target Components**: `alexandria-daemon/src/discovery.rs`, `alexandria-core/src/store.rs`

---

## 1. Executive Summary & Real-World Multi-Module Patterns

An empirical audit of the 4 local Motorola Android PoC repositories (`AudioRecorder`, `FMRadio`, `MotoAssistPremium`, and `motoDolbyUI`) under `/home/pedrogoncalves/projects/` uncovered **7 real-world multi-module patterns** that a deterministic **Clone-to-Onboard Discovery** parser must handle:

1. **Nested Gradle Root (`application/settings.gradle`)**:
   - `FMRadio` and `MotoAssistPremium` place `settings.gradle` at `<repo_root>/settings.gradle`.
   - `AudioRecorder` and `motoDolbyUI` place their Gradle build root one directory deeper at `<repo_root>/application/settings.gradle` (`<repo_root>/application/build.gradle`, `<repo_root>/application/gradle.properties`).
2. **AGP 8+ Implicit `applicationId` Fallback to `namespace`**:
   - In Android Gradle Plugin (AGP) 8+, if a module applying `com.android.application` declares `namespace` without an explicit `defaultConfig.applicationId`, the final APK `applicationId` defaults to `namespace`.
   - Verified in `FMRadio/FMPlayer/build.gradle:107`: declares `namespace = 'com.motorola.fmplayer'` with no `applicationId` in `defaultConfig`.
3. **Product Flavor `applicationId` Overrides and `applicationIdSuffix` Variants**:
   - Full flavor override: `motoDolbyUI/application/app/build.gradle:108-111` defines product flavor `motoAudioEffects` with `applicationId "com.motorola.motoaudioeffects"`, overriding base `applicationId "com.motorola.dolby.dolbyui"`.
   - Flavor suffix: `FMRadio/FMPlayer/build.gradle:177` defines product flavor `prc` with `applicationIdSuffix ".prc"`, yielding effective package ID `com.motorola.fmplayer.prc`.
4. **Multiple `com.android.application` Modules per Repository**:
   - `AudioRecorder`: `:app` (`com.motorola.audiorecorder`) and `:recordingservice` (`com.motorola.audiorecorder.service`).
   - `FMRadio`: `:FMPlayer` (`com.motorola.fmplayer`), `:stubapp` (`com.motorola.fmplayer`), `:overlay-revvl` (`com.motorola.fmplayer.overlay.revvl`), and `:fmsdktestapp` (`com.motorola.android.fmsdktestapp`).
   - `MotoAssistPremium`: `:Assist` (`com.motorola.moto`) and `:testapp:app` (`com.motorola.assist.testapp`).
5. **Divergent Java/Kotlin Source Tree Package Prefixes**:
   - Crash stack traces frequently contain Java/Kotlin class symbols whose package prefixes do not match `applicationId` or `namespace`:
     - `MotoAssistPremium`: `com.motorola.assist.*` and `com.motorola.showtips.*` (while `applicationId` is `com.motorola.moto`).
     - `AudioRecorder`: `com.motorola.airecorder.*` and legacy `com.dimowner.audiorecorder.*` (while `applicationId` is `com.motorola.audiorecorder`).
     - `motoDolbyUI`: `com.dolby.dax.*` and `com.motorola.bose.bosecontract.*` (while `applicationId` is `com.motorola.dolby.dolbyui`).
     - `FMRadio`: `com.motorola.android.fmradio.*` and `com.motorola.fmservices.*` (while `applicationId` is `com.motorola.fmplayer`).
6. **Cross-Repository Shared Library Namespaces & Disambiguation**:
   - Both `AudioRecorder` (`:common:coreui`) and `motoDolbyUI` (`:common:coreui`) declare `namespace 'com.motorola.coreui'`.
   - Disambiguation requires a strict **Role Priority** (`application_id` > `companion_application_id` > `flavor_application_id` > `overlay_application_id` > `module_namespace` > `manifest_package` > `source_prefix`) combined with `crash_app_id_hint` affinity when resolving stack-trace symbols.
7. **Deterministic `primary_app_id` Election for Vault Partitioning**:
   - Each repository must deterministically elect a single canonical `primary_app_id` for its Akashic Vault partition (`akashic/<primary_app_id>/`), ignoring test apps (`:testapp:app`, `:fmsdktestapp`), RRO overlays (`:overlay-revvl`), stub modules (`:stubapp`), and companion service modules (`:recordingservice`).

---

## 2. Gerrit Project & Repository Mapping Table

| Local Repo Directory | `.git/config` Remote `origin` URL | Extracted `gerrit_project` | Gradle Root (`settings.gradle`) | Canonical `primary_app_id` | Akashic Vault Partition |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `/home/pedrogoncalves/projects/AudioRecorder` | `ssh://pedrogoncalves@gerrit.mot.com:29418/home/repo/dev/platform/packages/apps/AudioRecorder` | `platform/packages/apps/AudioRecorder` | `application/settings.gradle` | `com.motorola.audiorecorder` | `akashic/com.motorola.audiorecorder` |
| `/home/pedrogoncalves/projects/FMRadio` | `ssh://pedrogoncalves@gerrit.mot.com:29418/home/repo/dev/platform/vendor/motorola/packages/apps/FMRadio` | `platform/vendor/motorola/packages/apps/FMRadio` | `settings.gradle` | `com.motorola.fmplayer` | `akashic/com.motorola.fmplayer` |
| `/home/pedrogoncalves/projects/MotoAssistPremium` | `ssh://pedrogoncalves@gerrit.mot.com:29418/home/repo/dev/platform/packages/apps/MotoAssistPremium` | `platform/packages/apps/MotoAssistPremium` | `settings.gradle` | `com.motorola.moto` | `akashic/com.motorola.moto` |
| `/home/pedrogoncalves/projects/motoDolbyUI` | `ssh://pedrogoncalves@gerrit.mot.com:29418/home/repo/dev/platform/vendor/motorola/packages/apps/motoDolbyUI` | `platform/vendor/motorola/packages/apps/motoDolbyUI` | `application/settings.gradle` | `com.motorola.dolby.dolbyui` | `akashic/com.motorola.dolby.dolbyui` |

---

## 3. Complete Module & Package Inventory Across the 4 PoC Repositories

### 3.1 `AudioRecorder` (`application/settings.gradle`)
- Root properties (`application/gradle.properties`): `PACKAGE_NAME = com.motorola.audiorecorder`

| Module | Plugin | `namespace` | `applicationId` / Flavors | `AndroidManifest.xml` `package` | Source Package Roots (`src/main/java` or `kotlin`) |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `:app` | `com.android.application` | `com.motorola.audiorecorder` | `com.motorola.audiorecorder` (`project.PACKAGE_NAME`) | *(none)* | `com.motorola.audiorecorder`, `com.dimowner.audiorecorder` |
| `:recordingservice` | `com.android.application` | `com.motorola.audiorecorder.service` | `com.motorola.audiorecorder.service` | *(none)* | `com.motorola.audiorecorder.service` |
| `:airecorder` | `com.android.library` | `com.motorola.airecorder` | *(library)* | *(none)* | `com.motorola.airecorder` |
| `:common:coreui` | `com.android.library` | `com.motorola.coreui` | *(library)* | *(none)* | `com.motorola.coreui` |
| `:common:SharedLibrary` | `com.android.library` | `com.motorola.audiorecorder.shared` | *(library)* | *(none)* | `com.motorola.audiorecorder.shared` |

### 3.2 `FMRadio` (`settings.gradle`)

| Module | Plugin | `namespace` | `applicationId` / Flavors | `AndroidManifest.xml` `package` | Source Package Roots |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `:FMPlayer` | `com.android.application` | `com.motorola.fmplayer` | `com.motorola.fmplayer` (AGP 8+ fallback to `namespace`), `com.motorola.fmplayer.prc` (`prc` flavor `applicationIdSuffix ".prc"`) | `com.motorola.fmplayer` | `com.motorola.fmplayer` |
| `:stubapp` | `com.android.application` | `com.motorola.fmplayer` | `com.motorola.fmplayer` | `com.motorola.fmplayer` | `com.motorola.fmplayer` |
| `:overlay-revvl` | `com.android.application` | `com.motorola.fmplayer.overlay.revvl` | `com.motorola.fmplayer.overlay.revvl` | `com.motorola.fmplayer.overlay.revvl` (RRO `targetPackage="com.motorola.fmplayer"`) | *(res only)* |
| `:fmsdktestapp` | `com.android.application` | *(none)* | `com.motorola.android.fmsdktestapp` | `com.motorola.android.fmsdktestapp` | `com.motorola.android.fmsdktestapp` |
| `:FMService` | `com.android.library` | `com.motorola.fmservices` | *(library)* | `com.motorola.fmservices` | `com.motorola.fmservices` |
| `:fmsdk` | `com.android.library` | `com.motorola.android.fmradio` | *(library)* | `com.motorola.android.fmradio` | `com.motorola.android.fmradio` |
| `:fmcustom` | `com.android.library` | `com.motorola.fmcustom` | *(library)* | `com.motorola.fmcustom` | `com.motorola.fmcustom` |

### 3.3 `MotoAssistPremium` (`settings.gradle`)
- Root `build.gradle`: `ext.packageName = 'com.motorola.moto'`

| Module | Plugin | `namespace` | `applicationId` / Flavors | `AndroidManifest.xml` `package` | Source Package Roots |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `:Assist` | `com.android.application` | `com.motorola.moto` (`rootProject.ext.packageName`) | `com.motorola.moto` (`rootProject.ext.packageName`) | `com.motorola.moto` | `com.motorola.assist`, `com.motorola.moto` |
| `:sdk` | `com.android.library` | `com.motorola.assist.sdk` | *(library)* | `com.motorola.assist.sdk` | `com.motorola.assist.sdk` |
| `:ShowTips` | `com.android.library` | `com.motorola.showtips` | *(library)* | `com.motorola.showtips` | `com.motorola.showtips` |
| `:testapp:app` | `com.android.application` | `com.motorola.assist.testapp` | `com.motorola.assist.testapp` | *(none)* | `com.motorola.assist.testapp` |

### 3.4 `motoDolbyUI` (`application/settings.gradle`)

| Module | Plugin | `namespace` | `applicationId` / Flavors | `AndroidManifest.xml` `package` | Source Package Roots |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `:app` | `com.android.application` | `com.motorola.dolby.dolbyui` | `com.motorola.dolby.dolbyui` (base), `com.motorola.motoaudioeffects` (`motoAudioEffects` flavor override) | `com.motorola.dolby.dolbyui` | `com.motorola.dolby.dolbyui`, `com.dolby.dax` |
| `:common:coreui` | `com.android.library` | `com.motorola.coreui` | *(library)* | `com.motorola.coreui` | `com.motorola.coreui` |
| `:bosecontract` | `com.android.library` | `com.motorola.bose.bosecontract` | *(library)* | `com.motorola.bose.bosecontract` | `com.motorola.bose.bosecontract` |

---

## 4. Five-Pass Static Discovery Algorithm (`alexandria-daemon/src/discovery.rs`)

Because `gradlew` invocation is slow (10–60s cold daemon startup) and requires matching local Android SDK/JDK installations, `alexandria-daemon` uses a **zero-execution 5-pass static extractor**:

### Pass 1: Git Remote & Gradle Root Discovery
1. Parse `<repo_root>/.git/config` for `[remote "origin"] url`.
   - Strip `/home/repo/dev/` or `ssh://[^/]+/` and optional `.git` suffix to normalize `gerrit_project` (e.g., `platform/packages/apps/AudioRecorder`).
2. Locate `settings.gradle` or `settings.gradle.kts` at `<repo_root>/settings.gradle*` or `<repo_root>/application/settings.gradle*`. Set `gradle_root` to its parent directory.
3. Extract `include` directives (`include ':app', ':common:coreui'` or `include(":app")`) and optional `project(':foo').projectDir = new File(...)` mappings to resolve module directories.

### Pass 2: Root Variable & Property Interpolation Table
1. Parse `<gradle_root>/gradle.properties` for key-value pairs (e.g., `PACKAGE_NAME=com.motorola.audiorecorder`).
2. Parse `<gradle_root>/build.gradle*` for `ext.<key> = '<val>'` and `ext { <key> = '<val>' }` (e.g., `ext.packageName = 'com.motorola.moto'`).
3. Store in a `HashMap<String, String>` to interpolate `project.<KEY>`, `rootProject.ext.<KEY>`, and `${<KEY>}` references in module Gradle files.

### Pass 3: Module `build.gradle` / `build.gradle.kts` Extraction
For each included module directory:
1. Detect module plugin kind:
   - `com.android.application` or `id("com.android.application")` -> `ModuleKind::Application`
   - `com.android.library` or `id("com.android.library")` -> `ModuleKind::Library`
2. Extract `namespace` (interpolating variables from Pass 2).
3. Extract `defaultConfig` `applicationId` and `applicationIdSuffix`.
   - **AGP 8+ Fallback Rule**: If `ModuleKind::Application` and `defaultConfig.applicationId` is absent, set base `applicationId = namespace`.
4. Extract `productFlavors` blocks:
   - If a flavor specifies `applicationId "<override>"`, emit a `flavor_application_id` entry for `<override>`.
   - If a flavor specifies `applicationIdSuffix "<suffix>"`, emit a `flavor_application_id` entry for `<base_app_id><suffix>`.

### Pass 4: `AndroidManifest.xml` Extraction
Scan `src/main/AndroidManifest.xml` (and flavor manifests `src/*/AndroidManifest.xml`) per module:
1. Extract `<manifest package="...">` attribute (`manifest_package`).
2. Extract `<overlay android:targetPackage="..."/>` (`is_overlay = true`).
3. If `ModuleKind::Application` still lacks a base `applicationId` after Pass 3 (legacy AGP < 7 without `namespace`), fall back to `<manifest package="...">`.

### Pass 5: Source Tree Package Prefix Extraction & Primary App ID Election
1. Walk `src/main/java` and `src/main/kotlin` in each module.
2. Read the `package <pkg>;` header from the shallowest `.java` / `.kt` files in each top-level domain subtree (e.g., `com/motorola/assist`, `com/dimowner/audiorecorder`, `com/dolby/dax`) and emit `source_prefix` entries.
3. **Deterministic `primary_app_id` Election**:
   Score each `ModuleKind::Application` candidate in the repository:
   - Base score: `+100`
   - Module name is `:app` or matches repo directory name case-insensitively (`:FMPlayer` in `FMRadio`, `:Assist` in `MotoAssistPremium`): `+50`
   - Module name or path contains `test`, `stub`, `sample`, `demo`: `-200`
   - Module is an RRO overlay (`<overlay` in Manifest or module name contains `overlay`): `-200`
   - Module name ends with `service` or `applicationId` ends with `.service`: `-40` (classifies as `companion_application_id`)
   - Highest-scoring module's base `applicationId` becomes `primary_app_id`, and `vault_partition` is set to `akashic/<primary_app_id>`.

---

## 5. SQLite Schema (`alexandria-core/src/store.rs`)

Add two tables to Alexandria's registry database (`~/.local/share/alexandria/registry.db` or the workspace index DB):

```sql
CREATE TABLE IF NOT EXISTS android_repositories (
    repo_name         TEXT PRIMARY KEY,
    local_root_path   TEXT NOT NULL UNIQUE,
    gradle_root_path  TEXT NOT NULL,
    gerrit_project    TEXT NOT NULL UNIQUE,
    primary_app_id    TEXT NOT NULL,
    vault_partition   TEXT NOT NULL,
    code_graph_db     TEXT NOT NULL,
    last_indexed_at   INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS android_package_index (
    package_id        TEXT NOT NULL,
    repo_name         TEXT NOT NULL REFERENCES android_repositories(repo_name) ON DELETE CASCADE,
    module_gradle_path TEXT NOT NULL,
    role              TEXT NOT NULL,
    role_priority     INTEGER NOT NULL,
    flavor_name       TEXT NOT NULL DEFAULT '',
    is_primary_app    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (package_id, repo_name, module_gradle_path, role, flavor_name)
);

CREATE INDEX IF NOT EXISTS idx_package_index_pkg
    ON android_package_index(package_id, role_priority ASC);
```

### Role Priority Mapping
| `role` | `role_priority` | Description |
| :--- | :---: | :--- |
| `application_id` | `1` | Canonical primary `com.android.application` `applicationId` |
| `companion_application_id` | `2` | Companion service/secondary APK in the same repo (e.g., `com.motorola.audiorecorder.service`) |
| `flavor_application_id` | `3` | Product flavor override or suffix (e.g., `com.motorola.motoaudioeffects`, `com.motorola.fmplayer.prc`) |
| `overlay_application_id` | `4` | RRO overlay package (`com.motorola.fmplayer.overlay.revvl`) |
| `module_namespace` | `5` | Library or app module `namespace` |
| `manifest_package` | `6` | `AndroidManifest.xml` `package` attribute |
| `source_prefix` | `7` | Java/Kotlin source root package (`com.motorola.assist`, `com.dolby.dax`, etc.) |
| `test_application_id` | `8` | Test application (`com.motorola.assist.testapp`, `com.motorola.android.fmsdktestapp`) |

---

## 6. Deterministic Resolution Queries

### 6.1 `resolve_app_id(crash_app_id: &str)`
Resolves an exact Android process/package name from a Bug2Go or MotoJira incident to its owning repository and Vault partition:

```sql
SELECT
    r.repo_name,
    r.local_root_path,
    r.gerrit_project,
    r.primary_app_id,
    r.vault_partition,
    r.code_graph_db,
    p.module_gradle_path,
    p.role
FROM android_package_index p
JOIN android_repositories r ON r.repo_name = p.repo_name
WHERE p.package_id = ?1
ORDER BY p.role_priority ASC, p.is_primary_app DESC
LIMIT 1;
```

### 6.2 `resolve_stack_symbol(fqcn: &str, crash_app_id_hint: Option<&str>)`
Resolves a fully-qualified class name from a Java/Kotlin exception frame (e.g., `com.motorola.assist.engine.AssistService.onStartCommand` or `com.motorola.coreui.widget.MotoToolbar.inflate`) using longest-prefix match + `crash_app_id_hint` affinity + `role_priority`:

```sql
SELECT
    r.repo_name,
    r.local_root_path,
    r.gerrit_project,
    r.primary_app_id,
    r.vault_partition,
    r.code_graph_db,
    p.package_id,
    p.module_gradle_path,
    p.role,
    LENGTH(p.package_id) AS prefix_len
FROM android_package_index p
JOIN android_repositories r ON r.repo_name = p.repo_name
WHERE (?1 = p.package_id OR ?1 LIKE (p.package_id || '.%'))
ORDER BY
    CASE
        WHEN ?2 IS NOT NULL AND (
            r.primary_app_id = ?2
            OR EXISTS (
                SELECT 1 FROM android_package_index p2
                WHERE p2.repo_name = r.repo_name
                  AND p2.package_id = ?2
                  AND p2.role_priority <= 4
            )
        ) THEN 0
        ELSE 1
    END ASC,
    prefix_len DESC,
    p.role_priority ASC
LIMIT 1;
```

This guarantees that:
- `com.motorola.assist.*` resolves deterministically to `MotoAssistPremium` (`akashic/com.motorola.moto`).
- `com.dolby.dax.*` resolves deterministically to `motoDolbyUI` (`akashic/com.motorola.dolby.dolbyui`).
- Shared library frames in `com.motorola.coreui.*` resolve to `AudioRecorder` when `crash_app_id_hint = Some("com.motorola.audiorecorder")` and to `motoDolbyUI` when `crash_app_id_hint = Some("com.motorola.dolby.dolbyui")` or `Some("com.motorola.motoaudioeffects")`.
