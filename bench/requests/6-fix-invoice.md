---
id: fix-invoice
title: Fix an invoice calculation without changing its interface
kind: project
fixture: invoice
proof: invoice
---
The invoice total in invoice.mjs charges for one item when quantity is zero. Fix total(lines), keeping its named export and integer-cent return value. Each line has quantity and unitCents: both must be nonnegative safe integers; reject invalid values by throwing. An empty invoice totals zero. Keep README.md unchanged, and add a regression test I can run with node --test. No dependencies.
