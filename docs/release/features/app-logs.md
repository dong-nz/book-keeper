# App logs

> **Status: ✅ Shipped in v0.0.11** — verified by running the app.

Book Keeper keeps a simple record of things that go wrong — a plain text file you can open whenever you want to know what happened, or to include it when reporting a problem.

## Where the log is

The log lives in Book Keeper's own data folder — the same folder that holds your index (you can see it under **Settings ▸ Settings**, in the *Data location* row). The log file sits in a **logs** subfolder there, in a file named `app.log`. It's an ordinary text file: any text reader can open it.

The quickest way there: the Settings window has a **Log** section that shows the exact path and carries an **Open log file** button. The app never shows log content inside its own windows — opening the file is always your decision.

## What gets recorded

By default (**Error** level) the log records problems:

- anything the app told you failed — a message you saw in a notification or a failed action;
- background problems you might not see: a file that failed while importing, a cover that couldn't be drawn, an update check that failed.

You can switch the level to **Info**, which additionally records the app's normal activity: starting and closing, import runs with their counts, reading windows opening and closing.

The level is remembered, takes effect the moment you change it, and works the same in every window — including the Settings window opened from a reading window.

What the log never holds: the contents of your documents, or your passwords. It records paths and application events — which file, which action — never what's inside the files.

## The log stays small

The log file has a size limit. When it fills up, the current file is set aside as `app.log.1` and a fresh one starts — at most two files, ever. Old history is dropped rather than archived.

## Checking or changing it

**Settings ▸ Settings ▸ Log:**

- **Logging level** — Error (the starting point) or Info. One press changes it, everywhere, immediately.
- The log file's exact path, shown as selectable text.
- **Open log file** — opens it in whatever application your system uses for text files. On a fresh install, where nothing has gone wrong yet, this creates an empty log file rather than failing.

## What this does not do

- There is no log viewer inside the app, and no search or filter — it's a plain text file, opened by you.
- The log is not a history of what you did with your documents; it's a record of application events.
- Nothing is sent anywhere: the log stays on your computer.

> **Verified behavior.** The Error level records failed actions exactly as the app reported them; switching to Info immediately adds the activity lines with no restart, in the library window and in a reading window's Settings alike; *Open log file* opens the real file (and creates it on a clean install); the path row shows the real path; a filled log rolls over to a single `.1` file and never keeps more than two; and a log write that cannot happen (a folder that can't be written) never turns into a failure you'd notice in the app.
