# Catalog audio runtimes

The Audio tools filter extends the existing catalog, its background actions, switches and Undo. Cards and details show
the original source, code and weight licences, complete download size for this OS, and required video memory for CUDA
items. Licence notices appear before install: `*-NC-*` in either licence shows the personal/non-commercial notice;
CC-BY shows attribution. MIT has neither notice. The same notices and uv prerequisite appear in the five screen languages.
Runtime metadata precedes the final state row so the existing bottom action never covers a licence notice.

`tools.json` has a `runtimes` section with the ordinary item identity and a strict runtime descriptor. Missing SHA-256 or
size, unknown descriptor fields, unsafe commands and oversized files fail its load checks. Supplementary
`runtime-bundles.json` pins each weight, dictionary and compatible wheel; `runtime-locks/` carries uv-generated hashed
requirements. Integrity metadata in `runtime-wheel-files.json` authenticates installed package files without relying on
mutable local receipts. No third-party executable, package, model or sound ships with Faber. UTMOSv2 records MIT for its
checkpoint and Apache-2.0 for its [wav2vec2 backbone](https://huggingface.co/facebook/wav2vec2-base).

Downloads use a leased store temporary copy. Every byte count and SHA-256 is checked before extraction or uv. Ordinary
runtime files are capped at 400 MiB; CUDA wheels and model assets at 3 GiB each, extracted Python environments at 12 GiB,
with bounded file counts and depth. An integrity failure removes the download and reports `error.tools.digest`.
Python installation resolves the user's native uv through the existing CLI resolver, creates a private relocatable
Python 3.12 environment, then runs offline `uv pip install --require-hashes -r requirements.txt` against checked wheels.
The small pure-Python ANTLR dependency has no official wheel: its checked source is extracted as an import library;
its setup script is never executed. Chatterbox's tokenizer model is also pinned and loaded locally, without a first-run download. Faber neither installs uv/Python nor executes package build hooks. Chatterbox keeps its source wheel's original
distribution metadata; curated inference locks omit the Gradio demo and use NumPy 1.26.4 for Python 3.12 rather than
the upstream demo metadata's NumPy <1.26 pin.

Runtime folders use the existing store lock, ledger, selection and removal/replacement journals. Off moves the whole
folder outside the active root; On verifies it again; Remove moves it to the journal's bounded Undo copy, then expiry
deletes the whole folder. Crash recovery uses the same leased staging sweeper and runtime-sized journal limits.
Kits copy only a tiny instruction file, never an environment or model. Full content verification runs at install,
when requested, and once per catalog preparation. Subsequent copies and launches compare a bounded cache entry with
all path names and file identity, size, modification and change times, then check the interpreter and launcher again.
Changing bytes and restoring the old modification time invalidates the launch cache. The cache holds at most 64 small
entries; immutable wheel metadata is indexed with borrowed JSON slices, parsing only each selected wheel transiently.
Runtime instruction snapshots are copies, so no hard link changes the integrity identity of their original files.

Python launches with `-I -B`. Only Chatterbox adds its exact checked ANTLR library; UTMOSv2 adds only its pinned source
folder. Unknown root and weight files, unexpected package files and bytecode are rejected. Interpreter pins are recorded
once at installation in engine-owned `tools/runtime-pins/` metadata outside the mutable runtime. Each content-addressed
snapshot is bound to the item, revision and source digest; the runtime holds only a reference. Prior snapshots remain
available to Undo. Snapshots are written and synced to a temporary file before atomic publication; retries verify an
existing snapshot's digest and replace incomplete or corrupt copies. The catalog ledger, native uv/Python installation and external install metadata are trusted; changing
only runtime files or their local reference cannot authorize another interpreter. Verification never regenerates a venv
or needs uv. An inconclusive check blocks delivery but preserves the installed or disabled copy for explicit repair;
an updated external system Python likewise requires repair without discarding its environment. Read and hash I/O
errors remain inconclusive, including package and archive checks; only missing or unexpected files and content
mismatches permit pruning the copy.

Extraction and uv run outside the global store lock; publication rechecks roots, switches, ledger and replacement
consent after reacquiring it. uv has a five-minute deadline with kill and reap, no inherited UV/PYTHON overrides, and
only an explicit host-environment allow-list. Python replacement receipts preserve only uv-shaped interpreter and
lib64 links, fingerprinting targets without following them. Downloads already owned by staging are renamed into the
installation rather than copied, and a conservative free-space check precedes download. No process stays alive.

Available CUDA memory is cached only after a successful reading; an unknown result is retried. Windows uses DXGI
local-memory budgets for NVIDIA adapters; other systems resolve an absolute native `nvidia-smi` from absolute PATH
entries. The minimum available memory across adapters is conservative for CUDA's default device. Unknown or insufficient
memory warns on the existing card/detail and prevents installation and kit selection. Windows/Linux
Python environments target x86-64; processor Python on macOS targets Apple silicon because CTranslate2 does not publish
the selected Intel Mac wheel. FluidSynth is Windows x86-64 only: no official Linux/macOS desktop binary is offered.
Data-only SoundFonts and packs work on all three desktop systems. GPU tools use CUDA, not Metal.

Non-Git artifacts keep their SHA-256 prefix as the existing 40-character ledger revision; their full SHA-256 and exact
version/release URL remain the integrity authority. The card shows the complete per-platform download cost, including
weights and wheels, not just the small Python package.

The narration launcher accepts a UTF-8 `--text-file`, or positional text after `--`, and validates the WAV extension
and an existing writable output directory before loading a model.
