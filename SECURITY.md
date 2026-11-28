# Security policy

## Report a vulnerability

Please report it privately through GitHub: open the repository's **Security** tab and choose **Report a vulnerability**
(private vulnerability reporting, which is on). Do not open a public issue or pull request for it.

Include what you found, the Faber version, your system, and the steps to reproduce it. A proof of concept helps but is
not required.

## Scope

In scope:

- the app: the screen, the Tauri shell and the fixed list of commands it exposes;
- the engine, including how it starts the AI command-line tools and stores conversations;
- the updater and the release signing, including the signature check on updates.

Out of scope: vulnerabilities in the official AI command-line tools themselves (report them to their vendors), in
third-party tools Faber downloads from their original source, and attacks that need an already compromised computer.

Faber never reads or stores the tools' sign-in credentials. If you find a way it could, that is a high-priority report.

## What to expect

Faber is maintained by one person, so these are goals, not guarantees: an acknowledgement within about 7 days, a first
assessment within about 14 days, and a fix released as soon as it is proven. You will be told when it ships, and credited
if you want to be.

Only the latest release is supported: the app updates itself, so older versions are not patched.
