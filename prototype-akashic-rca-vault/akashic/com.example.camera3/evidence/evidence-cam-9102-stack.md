---
id: evidence-cam-9102-stack
title: "Stack Trace Evidence for CAM-9102"
type: Evidence
status: active
valid_from: 2026-10-09
scope:
  app: com.example.camera3
  system: camera-framework
  device: falcon
  android_version: "14"
external_refs:
  jira_key: CAM-9102
  bugreport_id: BUG-882194
relations: []
---

# Evidence: CAM-9102 Stack Trace

## Symbolicated Crash Frame
```
#00 0x00042f10 in android::CameraDeviceClient::notifyError at CameraDeviceClient.cpp:412
#01 0x00038b24 in android::CameraDeviceClient::onCaptureErrorLocked at CameraDeviceClient.cpp:528
```
SIGSEGV due to null pointer dereference on `mResultExtras`.
