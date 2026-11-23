# Installing the Antigravity CLI

How Antigravity installs `agy`, and how Faber does the same.

Read on 2026-11-23 from antigravity.google (/download, /docs/cli/install) and the scripts they name. Windows:
`irm https://antigravity.google/cli/install.ps1 | iex` (or `install.cmd`); macOS and Linux:
`curl -fsSL https://antigravity.google/cli/install.sh | bash`, into `~/.local/bin/agy`. `install.ps1`:

1. stops if `%LOCALAPPDATA%\agy\bin\agy.exe` exists (the CLI updates itself);
2. reads `https://antigravity-cli-auto-updater-974169037036.us-central1.run.app/manifests/windows_amd64.json` (or
   `windows_arm64.json`): `{version, url, sha512}`, the `url` under
   `https://storage.googleapis.com/antigravity-public/antigravity-cli/`;
3. downloads it, refuses it unless its SHA-512 matches, and copies it to `%LOCALAPPDATA%\agy\bin\agy.exe`;
4. runs `agy.exe install` (PATH and shell settings), ignoring its failure.

Faber's Install (`engine-store/src/install.rs`) does the same without PowerShell. Of Cloud Storage only that folder is
official, redirects included; the version holds only letters, digits and `.-+`; an existing `agy.exe` is never written over;
`agy.exe install` runs with no input, hidden, ended after 60 s. Outside Windows, as for the other two, the card offers the
official page.
