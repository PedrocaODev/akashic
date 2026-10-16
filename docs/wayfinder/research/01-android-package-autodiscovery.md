# Research Report: Android Package-to-Repository Auto-Discovery across Multi-Module Gradle and Manifest Trees

**Ticket**: [Android Package-to-Repository Auto-Discovery across Multi-Module Gradle and Manifest Trees](../tickets/01-android-package-autodiscovery.md)  
**Date**: 2026-10-15  
**Target Components**: `alexandria-daemon/src/discovery.rs`, `alexandria-core/src/store.rs`

---

## 1. Executive Summary & Real-World Multi-Module Patterns

An empirical audit of the 4 local ExampleOEM Android PoC repositories (`SampleRecorder`, `SampleRadio`, `SampleAssist`, and `SampleAudioUI`) under `/home/pedrogoncalves/projects/` uncovered **7 real-world multi-module patterns** that a deterministic **Clone-to-Onboard Discovery** parser must handle:

1. **Nested Gradle Root (`application/settings.gradle`)**:
   - `SampleRadio` and `SampleAssist` place `settings.gradle` at `<repo_root>/settings.gradle`.
   - `SampleRecorder` and `SampleAudioUI` place their Gradle build root one directory deeper at `<repo_root>/application/settings.gradle` (`<repo_root>/application/build.gradle`, `<repo_root>/application/gradle.properties`).
2. **AGP 8+ Implicit `applicationId` Fallback to `namespace`**:
   - In Android Gradle Plugin (AGP) 8+, if a module applying `com.android.application` declares `namespace` without an explicit `defaultConfig.applicationId`, the final APK `applicationId` defaults to `namespace`.
   - Verified in `SampleRadio/SamplePlayer/build.gradle:107`: declares `namespace = 'com.example.sampleplayer'` with no `applicationId` in `defaultConfig`.
3. **Product Flavor `applicationId` Overrides and `applicationIdSuffix` Variants**:
   - Full flavor override: `SampleAudioUI/application/app/build.gradle:108-111` defines product flavor `motoAudioEffects` with `applicationId "com.example.audioeffects"`, overriding base `applicationId "com.example.audio.audioui"`.
   - Flavor suffix: `SampleRadio/SamplePlayer/build.gradle:177` defines product flavor `prc` with `applicationIdSuffix ".prc"`, yielding effective package ID `com.example.sampleplayer.prc`.
4. **Multiple `com.android.application` Modules per Repository**:
   - `SampleRecorder`: `:app` (`com.example.samplerecorder`) and `:recordingservice` (`com.example.samplerecorder.service`).
   - `SampleRadio`: `:SamplePlayer` (`com.example.sampleplayer`), `:stubapp` (`com.example.sampleplayer`), `:overlay-revvl` (`com.example.sampleplayer.overlay.revvl`), and `:fmsdktestapp` (`com.example.android.radiosdktestapp`).
   - `SampleAssist`: `:Assist` (`com.example.assist`) and `:testapp:app` (`com.example.assist.testapp`).
5. **Divergent Java/Kotlin Source Tree Package Prefixes**:
   - Crash stack traces frequently contain Java/Kotlin class symbols whose package prefixes do not match `applicationId` or `namespace`:
     - `SampleAssist`: `com.example.assist.*` and `com.example.showtips.*` (while `applicationId` is `com.example.assist`).
     - `SampleRecorder`: `com.example.airecorder.*` and legacy `com.example.legacyrecorder.*` (while `applicationId` is `com.example.samplerecorder`).
     - `SampleAudioUI`: `com.dolby.dax.*` and `com.example.audio.contract.*` (while `applicationId` is `com.example.audio.audioui`).
     - `SampleRadio`: `com.example.android.sampleradio.*` and `com.example.radioservices.*` (while `applicationId` is `com.example.sampleplayer`).
6. **Cross-Repository Shared Library Namespaces & Disambiguation**:
   - Both `SampleRecorder` (`:common:coreui`) and `SampleAudioUI` (`:common:coreui`) declare `namespace 'com.example.coreui'`.
   - Disambiguation requires a strict **Role Priority** (`application_id` > `companion_application_id` > `flavor_application_id` > `overlay_application_id` > `module_namespace` > `manifest_package` > `source_prefix`) combined with `crash_app_id_hint` affinity when resolving stack-trace symbols.
7. **Deterministic `primary_app_id` Election for Vault Partitioning**:
   - Each repository must deterministically elect a single canonical `primary_app_id` for its Akashic Vault partition (`akashic/<primary_app_id>/`), ignoring test apps (`:testapp:app`, `:fmsdktestapp`), RRO overlays (`:overlay-revvl`), stub modules (`:stubapp`), and companion service modules (`:recordingservice`).

---

## 2. Gerrit Project & Repository Mapping Table

| Local Repo Directory | `.git/config` Remote `origin` URL | Extracted `gerrit_project` | Gradle Root (`settings.gradle`) | Canonical `primary_app_id` | Akashic Vault Partition |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `/home/pedrogoncalves/projects/SampleRecorder` | `ssh://user@gerrit.example.com:29418/platform/packages/apps/SampleRecorder` | `platform/packages/apps/SampleRecorder` | `application/settings.gradle` | `com.example.samplerecorder` | `akashic/com.example.samplerecorder` |
| `/home/pedrogoncalves/projects/SampleRadio` | `ssh://user@gerrit.example.com:29418/platform/vendor/example/packages/apps/SampleRadio` | `platform/vendor/example/packages/apps/SampleRadio` | `settings.gradle` | `com.example.sampleplayer` | `akashic/com.example.sampleplayer` |
| `/home/pedrogoncalves/projects/SampleAssist` | `ssh://user@gerrit.example.com:29418/platform/packages/apps/SampleAssist` | `platform/packages/apps/SampleAssist` | `settings.gradle` | `com.example.assist` | `akashic/com.example.assist` |
| `/home/pedrogoncalves/projects/SampleAudioUI` | `ssh://user@gerrit.example.com:29418/platform/vendor/example/packages/apps/SampleAudioUI` | `platform/vendor/example/packages/apps/SampleAudioUI` | `application/settings.gradle` | `com.example.audio.audioui` | `akashic/com.example.audio.audioui` |

---

## 3. Complete Module & Package Inventory Across the 4 PoC Repositories

### 3.1 `SampleRecorder` (`application/settings.gradle`)
- Root properties (`application/gradle.properties`): `PACKAGE_NAME = com.example.samplerecorder`

| Module | Plugin | `namespace` | `applicationId` / Flavors | `AndroidManifest.xml` `package` | Source Package Roots (`src/main/java` or `kotlin`) |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `:app` | `com.android.application` | `com.example.samplerecorder` | `com.example.samplerecorder` (`project.PACKAGE_NAME`) | *(none)* | `com.example.samplerecorder`, `com.example.legacyrecorder` |
| `:recordingservice` | `com.android.application` | `com.example.samplerecorder.service` | `com.example.samplerecorder.service` | *(none)* | `com.example.samplerecorder.service` |
| `:airecorder` | `com.android.library` | `com.example.airecorder` | *(library)* | *(none)* | `com.example.airecorder` |
| `:common:coreui` | `com.android.library` | `com.example.coreui` | *(library)* | *(none)* | `com.example.coreui` |
| `:common:SharedLibrary` | `com.android.library` | `com.example.samplerecorder.shared` | *(library)* | *(none)* | `com.example.samplerecorder.shared` |

### 3.2 `SampleRadio` (`settings.gradle`)

| Module | Plugin | `namespace` | `applicationId` / Flavors | `AndroidManifest.xml` `package` | Source Package Roots |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `:SamplePlayer` | `com.android.application` | `com.example.sampleplayer` | `com.example.sampleplayer` (AGP 8+ fallback to `namespace`), `com.example.sampleplayer.prc` (`prc` flavor `applicationIdSuffix ".prc"`) | `com.example.sampleplayer` | `com.example.sampleplayer` |
| `:stubapp` | `com.android.application` | `com.example.sampleplayer` | `com.example.sampleplayer` | `com.example.sampleplayer` | `com.example.sampleplayer` |
| `:overlay-revvl` | `com.android.application` | `com.example.sampleplayer.overlay.revvl` | `com.example.sampleplayer.overlay.revvl` | `com.example.sampleplayer.overlay.revvl` (RRO `targetPackage="com.example.sampleplayer"`) | *(res only)* |
| `:fmsdktestapp` | `com.android.application` | *(none)* | `com.example.android.radiosdktestapp` | `com.example.android.radiosdktestapp` | `com.example.android.radiosdktestapp` |
| `:FMService` | `com.android.library` | `com.example.radioservices` | *(library)* | `com.example.radioservices` | `com.example.radioservices` |
| `:fmsdk` | `com.android.library` | `com.example.android.sampleradio` | *(library)* | `com.example.android.sampleradio` | `com.example.android.sampleradio` |
| `:fmcustom` | `com.android.library` | `com.example.radiocustom` | *(library)* | `com.example.radiocustom` | `com.example.radiocustom` |

### 3.3 `SampleAssist` (`settings.gradle`)
- Root `build.gradle`: `ext.packageName = 'com.example.assist'`

| Module | Plugin | `namespace` | `applicationId` / Flavors | `AndroidManifest.xml` `package` | Source Package Roots |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `:Assist` | `com.android.application` | `com.example.assist` (`rootProject.ext.packageName`) | `com.example.assist` (`rootProject.ext.packageName`) | `com.example.assist` | `com.example.assist`, `com.example.assist` |
| `:sdk` | `com.android.library` | `com.example.assist.sdk` | *(library)* | `com.example.assist.sdk` | `com.example.assist.sdk` |
| `:ShowTips` | `com.android.library` | `com.example.showtips` | *(library)* | `com.example.showtips` | `com.example.showtips` |
| `:testapp:app` | `com.android.application` | `com.example.assist.testapp` | `com.example.assist.testapp` | *(none)* | `com.example.assist.testapp` |

### 3.4 `SampleAudioUI` (`application/settings.gradle`)

| Module | Plugin | `namespace` | `applicationId` / Flavors | `AndroidManifest.xml` `package` | Source Package Roots |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `:app` | `com.android.application` | `com.example.audio.audioui` | `com.example.audio.audioui` (base), `com.example.audioeffects` (`motoAudioEffects` flavor override) | `com.example.audio.audioui` | `com.example.audio.audioui`, `com.dolby.dax` |
| `:common:coreui` | `com.android.library` | `com.example.coreui` | *(library)* | `com.example.coreui` | `com.example.coreui` |
| `:bosecontract` | `com.android.library` | `com.example.audio.contract` | *(library)* | `com.example.audio.contract` | `com.example.audio.contract` |

---

## 4. Five-Pass Static Discovery Algorithm (`alexandria-daemon/src/discovery.rs`)

Because `gradlew` invocation is slow (10–60s cold daemon startup) and requires matching local Android SDK/JDK installations, `alexandria-daemon` uses a **zero-execution 5-pass static extractor**:

### Pass 1: Git Remote & Gradle Root Discovery
1. Parse `<repo_root>/.git/config` for `[remote "origin"] url`.
   - Strip `/home/repo/dev/` or `ssh://[^/]+/` and optional `.git` suffix to normalize `gerrit_project` (e.g., `platform/packages/apps/SampleRecorder`).
2. Locate `settings.gradle` or `settings.gradle.kts` at `<repo_root>/settings.gradle*` or `<repo_root>/application/settings.gradle*`. Set `gradle_root` to its parent directory.
3. Extract `include` directives (`include ':app', ':common:coreui'` or `include(":app")`) and optional `project(':foo').projectDir = new File(...)` mappings to resolve module directories.

### Pass 2: Root Variable & Property Interpolation Table
1. Parse `<gradle_root>/gradle.properties` for key-value pairs (e.g., `PACKAGE_NAME=com.example.samplerecorder`).
2. Parse `<gradle_root>/build.gradle*` for `ext.<key> = '<val>'` and `ext { <key> = '<val>' }` (e.g., `ext.packageName = 'com.example.assist'`).
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
2. Read the `package <pkg>;` header from the shallowest `.java` / `.kt` files in each top-level domain subtree (e.g., `com/example/assist`, `com/example/legacyrecorder`, `com/dolby/dax`) and emit `source_prefix` entries.
3. **Deterministic `primary_app_id` Election**:
   Score each `ModuleKind::Application` candidate in the repository:
   - Base score: `+100`
   - Module name is `:app` or matches repo directory name case-insensitively (`:SamplePlayer` in `SampleRadio`, `:Assist` in `SampleAssist`): `+50`
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
| `companion_application_id` | `2` | Companion service/secondary APK in the same repo (e.g., `com.example.samplerecorder.service`) |
| `flavor_application_id` | `3` | Product flavor override or suffix (e.g., `com.example.audioeffects`, `com.example.sampleplayer.prc`) |
| `overlay_application_id` | `4` | RRO overlay package (`com.example.sampleplayer.overlay.revvl`) |
| `module_namespace` | `5` | Library or app module `namespace` |
| `manifest_package` | `6` | `AndroidManifest.xml` `package` attribute |
| `source_prefix` | `7` | Java/Kotlin source root package (`com.example.assist`, `com.dolby.dax`, etc.) |
| `test_application_id` | `8` | Test application (`com.example.assist.testapp`, `com.example.android.radiosdktestapp`) |

---

## 6. Deterministic Resolution Queries

### 6.1 `resolve_app_id(crash_app_id: &str)`
Resolves an exact Android process/package name from a BugReport or Jira incident to its owning repository and Vault partition:

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
Resolves a fully-qualified class name from a Java/Kotlin exception frame (e.g., `com.example.assist.engine.AssistService.onStartCommand` or `com.example.coreui.widget.SampleToolbar.inflate`) using longest-prefix match + `crash_app_id_hint` affinity + `role_priority`:

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
- `com.example.assist.*` resolves deterministically to `SampleAssist` (`akashic/com.example.assist`).
- `com.dolby.dax.*` resolves deterministically to `SampleAudioUI` (`akashic/com.example.audio.audioui`).
- Shared library frames in `com.example.coreui.*` resolve to `SampleRecorder` when `crash_app_id_hint = Some("com.example.samplerecorder")` and to `SampleAudioUI` when `crash_app_id_hint = Some("com.example.audio.audioui")` or `Some("com.example.audioeffects")`.
