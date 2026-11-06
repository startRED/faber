---
id: migrate-settings
title: Migrate preferences without losing existing data
kind: project
fixture: settings
proof: settings
---
Write migrate.mjs, run as node migrate.mjs <settings.json>. For version 1, move the string theme to appearance.theme, remove the old theme field and set version to 2. Preserve every other field, including unknown nested preferences. Version 1 has no appearance field. Version 2 must remain byte-for-byte unchanged on another run. Invalid JSON, unsupported versions, or a missing/non-string theme in version 1 must exit nonzero and leave the input bytes unchanged. Migrate the supplied settings.json too. Keep notes.txt unchanged. No dependencies.
