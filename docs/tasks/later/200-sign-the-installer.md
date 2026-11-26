# Sign the Windows installer and app with the owner's certificate

Area: Public self-update
Files: .github/workflows/, electron/forge.config.ts, scripts/, docs/dev/release.md, docs/product-map.md
Depends: none
Size: about 250 changed lines

Why: plan item A5 and the owner's answer 9 of 2026-11-25: the installer is signed before the next wave (eyes and hands), and the owner buys the certificate. An unsigned app that moves the mouse looks like malware to antivirus software, and today Windows warns on install.

Build:

- Signing in the release workflow from a secret the owner adds (certificate or signing service), for the installer, the app and the update helper, with a timestamp.
- A check in the release job that refuses to publish an unsigned build once the secret exists.
- The update path keeps working across the first signed version (the helper and the installer change publisher).

Leave out: buying or choosing the certificate (the owner's money); store listings.

Done: a release dry run shows signed files (`signtool verify /pa`), and the install-update proof passes from an unsigned to a signed version. Security and processes: two reviews from different vendors (R146). Starts only once the owner says the certificate exists.
