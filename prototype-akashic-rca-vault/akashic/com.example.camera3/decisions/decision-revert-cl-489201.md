---
id: decision-revert-cl-489201
title: "Revert of CL 489201 (Buffer Release Optimization)"
type: Decision
status: active
valid_from: 2026-10-10
scope:
  app: com.example.camera3
  system: camera-framework
external_refs:
  gerrit_cl: 490102
  reverted_cl: 489201
  jira_key: CAM-9102
relations:
  - type: supersedes
    target: ./decision-buffer-release-optimization.md
    evidence: ../evidence/evidence-cam-9102-stack.md
---

# Decision: Revert CL 489201

Reverted buffer release optimization due to fatal SIGSEGV in production soak (CAM-9102).
Restored defensive null-check on mResultExtras.
