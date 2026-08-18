# Changelog

Notable changes to this project, newest first. Versions follow
[Semantic Versioning](https://semver.org).

## [Unreleased]

## [1.5.0] - 2026-08-19

### Added

- **The app speaks more than one language.** Everything the interface says now
  comes from a language file rather than from the source, and it ships in
  English and French. Which one is used follows the system language by default
  and can be changed on the new **Settings** tab, taking effect at once with no
  restart.
- Language files are plain text and need no rebuild: copy
  `assets/lang/en.toml`, translate the right-hand side of each line, and drop
  it in the languages folder that Settings names and opens. Anything not yet
  translated falls back to English, so a file works from its first line, and a
  file whose `code` matches a built-in language replaces it. Whatever the
  parser could not read is listed on Settings with line numbers, and the rest
  of the file is still used. **Re-read the Files** picks up an edit without
  restarting.
- Reports are written in the language too — the headings, the column names and
  the month names in the CSV, Word and HTML files. The JSON report is
  deliberately left in English: its field names and month names are read by
  another program, which has no way to know which language wrote the file.
- **A light/dark choice.** The app still follows the operating system by
  default, but **Settings** can now pin it to light or dark regardless —
  changing the whole desktop is not a reasonable thing to ask of somebody who
  wants one window dimmer.
- A **Settings** tab, holding the two above and the startup update check. That
  checkbox was previously reachable only from inside the box offering an
  update, which is a place you cannot get back to once you have dismissed it.
- Settings now names any file in the languages folder that could not be used at
  all — unreadable, missing its `code` line, or claiming to be English, which
  is the one language a file may not replace — and says which and why. Those
  files were previously dropped in silence, leaving a translator looking at a
  picker their file was not in with nothing anywhere explaining it.

### Changed

- Both themes are now defined by the app as explicit colour pairs with the
  contrast worked out, rather than inherited from egui's defaults: every text
  colour reaches at least 4.5:1 against the surface it is drawn on, control
  outlines are real boundaries, and the keyboard focus ring is deliberately the
  loudest thing on screen. Supporting text is a shade rather than a whisper —
  egui's 60% default drops it below 4.5:1 on both themes.
- The release archives now carry a `languages/` folder holding the shipped
  language files, so somebody who downloaded a binary has a reference file to
  translate from without fetching the source.
- Releases no longer include a Linux download. Linux is still built and tested
  on every commit, and `cargo build --release` is all it takes there; a single
  dynamically linked binary cannot suit the distributions people actually run.

## [1.3.0] - 2026-08-15

### Security

- The SQLite database file and the folder holding it are now created
  private to the user who owns them, and an existing one left readable by
  an earlier version is tightened on open. On macOS this was already
  covered by `Application Support`; on Linux the file sat under
  `~/.local/share`, which on Debian and Ubuntu every other account on the
  machine can read.
- **Accept a certificate that does not match the host name** on the SQL
  Server tab now says what it does — it accepts any certificate, from any
  issuer, which is not what the MariaDB box of the same name did.

### Added

- Categories and spend entries can now be edited and deleted, not just
  added. A new **Entries** tab lists every entry for the year with
  Edit/Delete per row; **Categories** gained Rename/Delete per row.
- The app's first confirm-before-destroying prompts, for both of the above.
- **CA certificate file**, on the MariaDB tab, for a server whose
  certificate your own authority issued.
- A prompt before creating a SQLite database that is not there yet, so a
  typo in the path no longer opens an empty database and quietly reports
  success.

### Changed

- Deleting a category that still has spending in it is refused, with a
  message saying so, on all three backends.
- TLS is now rustls on all three platforms rather than each platform's
  own. The Linux binary no longer needs the system OpenSSL, so it runs on
  distributions whose version differs from the one it was built against,
  and certificates behave identically everywhere. Certificate authorities
  now come from a compiled-in list of the public ones rather than from the
  operating system: a server certificate issued in-house needs the new
  **CA certificate file** setting, or, on SQL Server, the accept-any box.
- A `~/…` path typed into **Database file** now means the home directory,
  as it does everywhere else, rather than a folder actually called `~` —
  which is the form the status bar displays.
- The SQL Server certificate setting is no longer hidden behind **Encrypt
  the connection**. SQL Server encrypts the login regardless, so its
  certificate was being checked even with that box clear, leaving a stock
  install unreachable with no visible way to allow it.

### Fixed

- `libssl-dev` is no longer needed to build on Linux.

## [1.2.0] - 2026-08-15

### Added

- SQL Server as a third database backend, alongside SQLite and MariaDB.
- Data on MariaDB and SQL Server is now scoped per login, so several
  people can share one server and each only ever sees their own
  categories and spending. An existing database is upgraded in place the
  first time anyone connects to it.
- Reference schemas and a starting grant statement for MariaDB and SQL
  Server, in `docs/`.

### Fixed

- Negative amounts (refunds, credits) in CSV exports were quoted as text
  by the formula-injection guard, which broke a spreadsheet's `SUM()`
  over that column.
- The SQL Server backend had no connect timeout, unlike MariaDB's five
  seconds — a host that dropped packets instead of refusing the
  connection could hang the "Connecting…" state forever.

## [1.1.0] - 2026-08-12

### Added

- Reports: a year's spending as CSV, Word (`.docx`), HTML or JSON, each
  shaped for what opens it — plain numbers and a locale-appropriate
  delimiter for a spreadsheet, a self-contained page for a browser,
  integer minor units for a program, an A4 layout with repeating headers
  for Word. All four are built from one reading of the year's entries.
- A startup check against GitHub for a newer release: downloads and
  installs nothing, can be turned off, and a dismissed version is not
  raised again.
- A release can be republished under the same tag — a re-tag, or a run
  restarted after one platform hiccups — updating the existing release
  in place instead of failing.

### Fixed

- Opening a MariaDB connection blocked the UI thread for up to its
  five-second timeout, so a misspelled hostname froze the window with no
  way to reach the Database tab to fix it. Connections now open on their
  own thread, the Database tab's buttons go quiet mid-attempt, and a
  failure names which database it was trying to reach.
- The window could sit on a stale frame after a background connection or
  update check finished, until the mouse happened to move.
- The audio device was opened at startup and held for the life of the
  process; it is now opened only the first time there is something to
  play.
- MySQL driver errors reached the status bar wrapped in their Rust type
  name (`DriverError { ... }`) rather than a message about the server.

### Security

- Text typed into a report is escaped in the three markup formats, and a
  description beginning with a spreadsheet-formula character is defused
  on the way into CSV.

### Accessibility

- The status colours (success green, error red) were only legible
  against a light background, falling to about 2.5:1 contrast in dark
  mode. Each now has a colour for both themes.

## [1.0.0] - 2026-08-12

Initial release.

### Added

- A two-pane app with Categories, Spending and Database tabs. Categories
  lists the year's totals with a button to add another; Spending is the
  entry form; Database chooses between a local SQLite file and a MariaDB
  server, switching over only once the new connection has been made and
  read from — a bad one leaves the working database in place.
- Money and dates follow the system locale throughout: currency symbol
  and placement, digit grouping (including the Indian lakh/crore
  convention), separators and date order, across about sixty regions,
  falling back to ISO dates and a generic currency sign otherwise.
  Amounts are stored as whole minor units, never as floating point.
- CI: clippy, the tests and a release build on Ubuntu, Apple silicon and
  Windows for every push; a `v*` tag publishes a GitHub Release with a
  binary for each platform plus a `SHA256SUMS`.

[Unreleased]: https://github.com/mediaswing/watchspend/compare/v1.5.0...HEAD
[1.5.0]: https://github.com/mediaswing/watchspend/compare/v1.3.0...v1.5.0
[1.3.0]: https://github.com/mediaswing/watchspend/compare/v1.2.0...v1.3.0
[1.2.0]: https://github.com/mediaswing/watchspend/compare/v1.1.0...v1.2.0
[1.1.0]: https://github.com/mediaswing/watchspend/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/mediaswing/watchspend/releases/tag/v1.0.0
