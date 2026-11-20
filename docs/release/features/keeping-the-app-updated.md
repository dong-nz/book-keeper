# Keeping the app updated

> **Status: ✅ Shipped in v0.0.10, updated in v0.0.11 and v0.0.13** — verified by running the app (v0.0.13 against a test release feed; the feed the app itself uses is the real published releases).

Book Keeper can now tell you when a newer version exists, and install it for you.

## A quiet check at startup

Every time Book Keeper starts, it quietly looks for a newer version. You notice nothing when there's nothing new, when the check can't reach its source, or while it's still running — the app opens at its usual speed either way.

When a newer version is found, a small notification appears in the bottom-right corner — the same quiet notification style the app already uses. It names the new version and shows that version's notes, and it carries one action: **Install**. Nothing is downloaded or changed until you press it.

## Checking yourself

The Settings menu — in the library window, and in any reading window — carries **Check for updates**. Pressing it checks right away and reports in the same quiet style:

- a newer version found → the notification with its notes and **Install**;
- nothing newer → a short *"You're up to date"*;
- the check can't be completed → a short, plain failure notice.

## Installing

Pressing **Install** downloads the new version, with a progress bar in the status bar you can watch. When the download is complete the app launches the installer and closes itself so the installation can run — you reopen Book Keeper when the installer is done, and your library, settings and starred documents are exactly as they were (they live in your own data folder, which the update does not touch).

## What this does not do

- It never installs anything by itself — **Install** is always your decision.
- It does not download anything during the startup check — only the version list is read.
- If no newer version is published, the check simply reports that. The check reads the application's official published releases; what it can find depends on what has been published.

> **Verified behavior.** All of the above was verified by running the app against a test release feed: the startup check finds a newer version and shows its notes with the Install action, or stays completely silent when there's nothing newer or the source is unreachable — startup is unaffected by the check: measured, the window came up in about 0.25 s with the check running against a reachable source and the same with an unreachable one; the manual check reports all three outcomes; the download shows real progress; and the install launched the installer and closed the app.

> **Amendment (2026-11-18, v0.0.11): the update feed is now the application's real published releases.** Before this release the check read a test feed — the feature was fully proven, but there was nothing real to find. Since v0.0.11 every published version of Book Keeper is released automatically, together with its update information, and the checks described above now read exactly that: the application's official published releases. The update from 0.0.10 to 0.0.11 is published with this release, so an installed 0.0.10 finds a newer version the very first time it checks. Nothing about what you do changed: the startup check, *Check for updates*, the notification, **Install** — all as described above.

> **Amendment (2026-11-20, v0.0.13): where the app is installed on Windows, and one cleanup note.** A fresh install goes into your own user account — no administrator needed. An update then stays exactly where the app already is: the installer returns to the location your installation used, so an app installed per-user keeps updating per-user. One machine situation can look odd: a computer that carried an older whole-device install (installed for all users by an early release) shows **two** Book Keeper entries in *Settings → Apps & features* — one under your account and one for the whole device. The one under your account is the live one; the whole-device entry is the stale leftover. Removing it once (*Apps & features → the whole-device Book Keeper → Uninstall*) leaves a single clean entry. Your library, settings and starred documents are untouched either way — they live in your own data folder, shared by both entries, which no install or update writes to.

> **Amendment (2026-11-20, v0.0.13): a newer version is presented as a dialog, and you can skip it.** (Verified by running the app against a test release feed; the checks themselves read the real published releases.) The availability notice described above is now an **update dialog**: it names the version you are running and the newer version, shows the newer version's release notes, and carries two actions — **Install** and **Skip this version**.
>
> - Closing the dialog without choosing defers it for this session only: the next start presents the same version again.
> - **Skip this version** remembers that exact version: from then on the startup check runs and asks nothing — it stays quiet for the version you skipped. It is not a refusal: *Check for updates* always re-presents the skipped version when you ask, and **Install** works from there as usual. A newer version than the one you skipped is presented normally.
> - **Install** and the download progress are unchanged. What happens after the download finished changes on Mac and Linux: there the app used to keep running on the old version until you restarted it yourself. Now a short *"Update installed"* message appears and the app **restarts itself** into the new version a moment later. On Windows the path was already as described above (the app closes, the installer runs, you reopen the app). On Linux the in-app update works for the AppImage edition; the ".deb" edition is updated by installing its new package.
>
> The startup check itself is unchanged: quiet when there is nothing newer or the source cannot be reached, and it never slows the app opening. Everything else — *Check for updates*, the up-to-date report, the plain failure notice — behaves as described above.
