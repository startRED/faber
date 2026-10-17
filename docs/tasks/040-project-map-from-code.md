# A project map built from the code (R146)

`docs/product/building.md`, "Coherence of a project's work": extractors list what a project has (settings, routes,
commands, texts, exports), each tied to the file it came from, with no AI-written overview, and the project's agent gets
the index through a Faber tool beside `verify` and looks details up. Start with what Faber's own repository needs
(Tauri commands, locale keys, stored settings, which `engine/tests/product_map.rs` already reads) and keep the
extractors language by language. Follow product-rules.md before anything a person sees.

Done when: the agent of a project can ask for its map and gets a short index with file links; Faber's own map matches
what `engine/tests/product_map.rs` checks; engine tests cover each extractor on fixtures; the product map has its row.
