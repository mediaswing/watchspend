# Changelog

Notable changes to this project, newest first. Versions follow
[Semantic Versioning](https://semver.org).

## [Unreleased]

## [1.3.0] - 2026-08-15

### Added

- Categories and spend entries can now be edited and deleted, not just
  added. A new **Entries** tab lists every entry for the year with
  Edit/Delete per row; **Categories** gained Rename/Delete per row.
- The app's first confirm-before-destroying prompts, for both of the above.

### Changed

- Deleting a category that still has spending in it is refused, with a
  message saying so, on all three backends.

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

[Unreleased]: https://github.com/mediaswing/watchspend/compare/v1.3.0...HEAD
[1.3.0]: https://github.com/mediaswing/watchspend/compare/v1.2.0...v1.3.0
[1.2.0]: https://github.com/mediaswing/watchspend/compare/v1.1.0...v1.2.0
[1.1.0]: https://github.com/mediaswing/watchspend/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/mediaswing/watchspend/releases/tag/v1.0.0
