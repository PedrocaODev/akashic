---
id: incident-motocam-9102
title: "SIGSEGV in CameraDeviceClient::notifyError"
type: Incident
status: active
valid_from: 2026-10-09
scope:
  app: com.motorola.camera3
  system: camera-framework
  device: falcon
  android_version: "14"
external_refs:
  jira_key: MOTOCAM-9102
  bug2go_id: B2G-882194
  gerrit_cl: 489201
relations:
  - type: supported_by
    target: ../evidence/evidence-motocam-9102-stack.md
  - type: caused_by
    target: ../decisions/decision-buffer-release-optimization.md
---

# Incident: MOTOCAM-9102

SIGSEGV crash observed on falcon devices running build U1UD34.22-12.
Root cause identified as missing null-check introduced in CL 489201.
