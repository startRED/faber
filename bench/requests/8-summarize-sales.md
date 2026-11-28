---
id: summarize-sales
title: Summarize a CSV export without changing the source
kind: project
fixture: sales
proof: sales
---
Write summarize.mjs, run as node summarize.mjs <file.csv>. Print one JSON object mapping each category to its total in integer cents. The header is exactly category,amount; amounts have a sign if negative and exactly two decimal places. Support LF and CRLF, quoted categories containing commas and escaped double quotes. Fields do not contain newlines. Refunds subtract, zero is valid, and a header-only file returns {}. Invalid headers, malformed quoting or invalid amounts must exit nonzero. Keep sales.csv byte-for-byte unchanged. No dependencies.
