# Generic Accounting System

[![CI](https://github.com/mediaswing/watchspend/actions/workflows/ci.yml/badge.svg)](https://github.com/mediaswing/watchspend/actions/workflows/ci.yml)

A small graphical budgeting app: put your spending into categories, and see
what each category has cost you so far this year. Written in Rust, with
[egui](https://github.com/emilk/egui) for the interface.

Everything you read or type — the currency symbol, where it sits, how digits
are grouped, the order of the parts of a date — comes from your system locale.
In the United Kingdom that means `£1,234.56` and `11/08/2026`; in Germany the
same figures are `1.234,56 €` and `11.08.2026`.

## Running it

```sh
cargo run --release
```

There is nothing to configure first: on the first run the app creates a SQLite
file under your data directory and starts with no categories in it.

## The window

Three tabs run down the left-hand side, and the pane on the right shows the one
you have picked.

**Categories** lists every category with the total spent in it this calendar
year — the name on the left, the amount right-aligned in your currency —
followed by a grand total. The **Add New Category** button beneath the table
opens a box asking for the new category's name. Names must be unique, and a
name that only differs in capitalisation is not a new name.

**Spending** is the form for recording what you spent: the date (which starts
on today), the category, the amount, and an optional description. Nothing is
recorded until every field makes sense, and if something does not, the form
says what and why.

**Database** chooses where all of this is kept — see below.

Every action either works or does not, and says so twice: a message along the
bottom of the window, and one of two sounds.

## Where the data is kept

**SQLite** (the default) is a single file, by default at:

| Platform | Path |
| --- | --- |
| macOS | `~/Library/Application Support/GenericAccountingSystem/accounts.sqlite` |
| Linux | `~/.local/share/GenericAccountingSystem/accounts.sqlite` |
| Windows | `%APPDATA%\GenericAccountingSystem\accounts.sqlite` |

You can point the app at a different file from the Database tab; it is created
if it is not there yet.

**MariaDB** (or MySQL) is the other option, for when several machines should
share one set of figures. Fill in the host, port, database, user name and
password, optionally turn on TLS, and use **Test Connection** before
committing to it. The two tables are created on first connection, so the user
needs `CREATE` as well as `SELECT` and `INSERT`.

The choice is remembered in `config.json` next to the default database file.
The MariaDB password is only written there if you tick the box that says so —
that file is plain text, kept readable by you alone, and no more secure than
that.

## Currencies and dates

The locale is read from the operating system. Around sixty regions are
described in `src/locale.rs`, covering currency, symbol placement, digit
grouping (including the Indian lakh/crore convention), decimal separator and
date order. A locale that is not in that table falls back to ISO dates and the
generic currency sign `¤` rather than guessing at a currency.

To see the app as another locale would:

```sh
GAS_LOCALE=de-DE cargo run
```

Amounts are stored as whole minor units — pence, cents, whole yen — never as
floating point, and each entry is stored with the currency it was recorded in.
If you move a database between machines with different locales, entries in the
other currency are counted and mentioned under the table rather than being
silently added to a total they do not belong in.

## Tests

```sh
cargo test
```

The tests cover the places where being wrong would be expensive: locale
formatting and parsing (including that `1,50` means different amounts in
different places, and that a date is never quietly reinterpreted), the SQLite
queries behind the totals, the MariaDB settings that are checked before any
connection is attempted, and that both sound files still decode.

## Building elsewhere

Every push is built and tested on Ubuntu, Apple silicon macOS and Windows by
[the CI workflow](.github/workflows/ci.yml), which leaves a release binary for
each as a downloadable artifact.

On Linux the window, the sound and the MariaDB TLS option need a few
development packages that the other two platforms already have:

```sh
sudo apt-get install libasound2-dev libssl-dev libwayland-dev \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libxkbcommon-dev libxkbcommon-x11-dev
```

## Licence

GNU General Public License, version 3 or later. The full text is in
[`LICENSE`](LICENSE); the short of it is that you may use, study, change and
share this program, and anything you distribute that is built from it has to
come with the same freedoms and its source.

The two asset licences below are separate from that and unaffected by it.

## Assets

- `assets/fonts/Ubuntu-Bold.ttf` — the interface typeface, under the Ubuntu
  Font Licence 1.0 (`assets/fonts/UBUNTU-FONT-LICENCE-1.0.txt`).
- `assets/sounds/success.wav`, `assets/sounds/error.wav` — the two cues, both
  CC0. Their origins are credited in `assets/sounds/CREDITS.txt`.

Both are compiled into the binary, so it has no runtime dependency on the
`assets` folder. If no audio device is available the app carries on in silence.
