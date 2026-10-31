# Faber counts, on the PC only, how often a result needs correcting after done

Area: Project memory
Files: engine/src/look.rs, engine/src/engine/plan.rs, docs/product-map.md
Depends: none
Size: about 150 changed lines

Why: slice 6 of `docs/superpowers/specs/2026-10-30-faber-method-2.md`. The bench says how the method does on fixed
requests; real use says how it does on the person's own.

Build: local counts only, never sent, like `look.jsonl`: a message that corrects or reopens work after a closing box said
done, and the time from a request to its accepted close. Kept per project, bounded, deleted with the project.

Done: engine tests that fail today count a correction after done and none before; the file stays bounded. Extends:
"Project memory". No what's-new entry. One review by the other vendor (R146).
