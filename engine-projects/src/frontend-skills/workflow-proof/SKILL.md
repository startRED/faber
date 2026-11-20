---
name: workflow-proof
description: Use when multi-stage software, Roblox Studio, Blender, interface or thumbnail work needs an adaptable workflow or focused evidence.
---

# Adapt the workflow and proof

Choose the next step from the outcome, starting artifact, constraints, available tools and evidence already gathered. Do not force a fixed sequence. Keep the artifact, inputs, tool version, check, result and limits together outside build caches when they must survive. Continue independent work when a check is blocked and report what remains unverified.

- Software: reproduce the failure or exercise the new behavior through its normal entry point. Run a focused check before and after the change.
  Confirm the expected test count or output and preserve a native command's exit code before printing a summary.
- Roblox Studio: playtest the changed behavior; for replication changes use a server and at least two clients, recording what each sees. [Testing modes](https://create.roblox.com/docs/studio/testing-modes)
- Blender to Roblox: use the [Khronos glTF Validator](https://github.com/KhronosGroup/glTF-Validator) for glTF exports. Check orientation, scale and materials in Studio with an asymmetric sample marked at the front. Use a disposable place and uncheck Upload to Roblox, which is enabled by default. Keep source, export and importer warnings. [Blender guidance](https://create.roblox.com/docs/art/blender) · [Importer](https://create.roblox.com/docs/studio/importer)
- Interfaces: use frontend-proof for visual and interaction evidence.
- Thumbnails: check the exported image against the platform's format and size guidance. Compare legibility, cropping and subject recognition at the same feed and phone sizes, using the actual image. An eligible published-video watch-time experiment requires the owner's authorization. [YouTube image guidance](https://support.google.com/youtube/answer/72431?hl=en) · [YouTube experiment](https://support.google.com/youtube/answer/16391400?hl=en-GB)

Resume from evidence, not assumptions. Bound retries by scope and a success condition; repeat only when there is a new result or changed evidence. Resolve conflicting evidence before widening changes. [Retry policies](https://docs.temporal.io/encyclopedia/retry-policies)
