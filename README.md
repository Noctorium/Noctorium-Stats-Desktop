# Noctorium Stats

Your Noctorium listening, counted, in a window of its own on Windows, macOS and Linux. Sign in with the
account Noctorium itself signs in to, and it shows what you have listened to over the last week, month,
quarter or year, or all of it: how many songs, how many different ones, how many artists and how many
hours; a bar for every day or month; which services it came from, in their own colours; your top songs and
artists; the hours of the day and the days of the week you listen most; what you played last; and how many
days in a row you have kept it up.

It is a companion to the player, not a part of it. The player records each song it plays to the Noctorium
account service as it plays it; this asks the service for the figures and draws them. Nothing is counted on
your machine, so the figures are the same here, on the phone app
([Noctorium-Stats-Mobile](https://github.com/Noctorium/Noctorium-Stats-Mobile)) and on the website. The
player is [Noctorium-Desktop](https://github.com/Noctorium/Noctorium-Desktop) and
[Noctorium-Mobile](https://github.com/Noctorium/Noctorium-Mobile); releases, and the installers that fetch
this with them, are [Noctorium-Installer](https://github.com/Noctorium/Noctorium-Installer).

## Using it

Open it and sign in, or make an account there and then. An account with nothing played yet says what to do
about it: open Noctorium, sign in to the same account, and play something. Choose a range at the top right;
**Refresh**, or F5, fetches the figures again. Everything that can be pointed at says what it is: a bar its
day and its plays, a wedge of the clock its hour.

The figures for each range are kept once fetched, so the window opens on the last ones straight away and
fetches newer ones behind them, and with no network it shows what it has and says how old it is. The days
and hours are your own: the service is told this machine's offset from UTC, so a song at eleven at night
belongs to that evening rather than to the next morning somewhere else.

**Sign out** forgets the sign-in and every figure kept with it. The sign-in lasts ninety days, after which
it asks again.

## Where it keeps things

| | Windows | macOS | Linux |
| --- | --- | --- | --- |
| The sign-in | Credential Manager | the login Keychain | the Secret Service (GNOME Keyring, KWallet) |
| Settings | `%APPDATA%\Noctorium Stats` | `~/Library/Application Support/Noctorium Stats` | `~/.config/noctorium-stats` |
| The figures last fetched | `%LOCALAPPDATA%\Noctorium Stats` | `~/Library/Caches/Noctorium Stats` | `~/.cache/noctorium-stats` |

The sign-in is a token that opens your account, so it goes into the system's own store for secrets, under
"Noctorium Stats". A desktop without one -- a bare window manager, say, with no keyring running -- still
gets signed in: the token goes into `sign-in.json` in the settings folder instead, readable only by you,
and the window says so at the bottom of the page. Settings hold the range you last looked at and the
address you last signed in with, never the token.

| Variable | |
| --- | --- |
| `NOCTORIUM_SERVICE_URL` | The account service to ask, rather than `https://noctorium-service.vercel.app`: a preview deployment, a service of your own, or a copy on this machine. The player reads the same one. A sign-in to one service is never offered to another. |
| `NOCTORIUM_STATS_HOME` | Keep the settings, the figures and the sign-in in this one folder, and leave the system's keyring alone. For trying it without touching anything of yours. |
| `NOCTORIUM_NO_UPDATE_CHECK` | Do not ask GitHub whether a newer release is out. |
| `GITHUB_TOKEN` | Raises GitHub's rate limit for that question, which an address shares with everyone behind it. |

## Updates

It is released with Noctorium, from the same tag, so it asks GitHub for the latest release of
Noctorium-Installer once at launch, on its own thread, and if that release is newer than itself says so at
the bottom of the window with a link to it. It downloads nothing. The installers fetch and check it along
with everything else, and the version in the window's About line is the release's.

## Building it

```bash
cargo test                 # everything that needs no display and no network
cargo build --release      # target/release/noctorium-stats, or noctorium-stats.exe
```

Rust 1.88 or newer. On Linux the window is built against the headers a desktop usually has already:

```bash
sudo apt-get install libxkbcommon-dev libwayland-dev libgl1-mesa-dev libx11-dev libxcursor-dev libxrandr-dev libxi-dev
```

The toolkit is [egui](https://github.com/emilk/egui) through eframe, drawn with OpenGL -- the same one, at
the same version, as the installer's window. HTTP is ureq with rustls, so there is no OpenSSL to find. The
icon is put into the Windows `.exe` by `build.rs`, with a version resource beside it, without a resource
compiler: MSVC's linker is handed a `.res` file and the GNU one a COFF object, both written there. Release
builds on Windows have no console window; debug builds keep one.

The window can be looked at without opening one. `cargo test --lib -- --ignored the_window_drawn` lays out
every screen -- signed out, each range wide and at the window's own size, a narrow window, an empty account,
loading, offline, an error -- and paints them into PNGs, in the folder `NOCTORIUM_RENDER_TO` names or in
`noctorium-stats-render` in the temporary folder. The tests that are not ignored lay every screen out too
and read back what a screen reader would be told.

Against a copy of the service, never the live one:

```bash
export NOCTORIUM_SERVICE_URL=http://localhost:3000 NOCTORIUM_TEST_EMAIL=... NOCTORIUM_TEST_PASSWORD=...
cargo test --test service -- --ignored                           # the client, every range
cargo test --lib -- --ignored the_window_against_a_service       # sign in through the form, draw what comes back
```

`tests/fixtures/` holds the service's answers for each range, captured from such a copy with made-up plays.

## Packaging, and the release

`packaging/package.sh <version> <platform>` builds one platform with the version built in and packs it as
the release ships it. It runs in bash on GitHub's own runners, Windows included, from the root of this
repository; the version is with or without its `v`, and pre-releases keep their suffix in the file's name.
It runs the program it built with `--version` and stops if the version it says is not the one asked for --
except for `macos-x64` on Apple silicon, which it does not try to run. Its last line on standard output is
the path of the file it made, and that is all it prints there; the build's own output goes to standard
error.

| Platform | Runner | Before it | Leaves |
| --- | --- | --- | --- |
| `windows-x64` | `windows-latest`, `shell: bash` | -- | `dist/noctorium-stats-<version>-windows-x64.zip`: `Noctorium Stats.exe`, with the icon in it and no console |
| `linux-x64` | `ubuntu-latest` | the headers above, and `desktop-file-utils` to check the `.desktop` file | `dist/noctorium-stats-<version>-linux-x64.tar.gz`: `noctorium-stats`, `noctorium-stats.desktop`, `noctorium-stats.png` |
| `macos-arm64` | `macos-latest` | -- | `dist/noctorium-stats-<version>-macos-arm64.zip`: `Noctorium Stats.app` |
| `macos-x64` | `macos-latest` | `rustup target add x86_64-apple-darwin` (the script tries it itself) | `dist/noctorium-stats-<version>-macos-x64.zip`: `Noctorium Stats.app` |

Every file is at the top of its archive, with no folder around it. The `.app` has the bundle id
`app.noctorium.stats`, the version in its `Info.plist`, an `.icns` made from the mark with `sips` and
`iconutil`, and an ad hoc signature; it is zipped with `ditto`, so the signature survives unzipping. It is
not notarised, so the first time it is opened with right-click, Open, as Noctorium.app is -- or the
quarantine flag is taken off by whatever installed it. The `.desktop` file's `Exec` and `Icon` name
`noctorium-stats` alone: whatever installs it puts the binary on PATH and the PNG among the icons, or
rewrites both.

In Noctorium-Installer's `release.yml`, one job per row, with this repository checked out at the release's
own tag beside the installer's -- which means the tag goes on this repository too, before the installer's:

```yaml
stats-desktop:
  strategy:
    fail-fast: false
    matrix:
      include:
        - { os: windows-latest, platform: windows-x64 }
        - { os: ubuntu-latest, platform: linux-x64 }
        - { os: macos-latest, platform: macos-arm64 }
        - { os: macos-latest, platform: macos-x64 }
  runs-on: ${{ matrix.os }}
  steps:
    - uses: actions/checkout@v4
      with:
        repository: Noctorium/Noctorium-Stats-Desktop
        ref: v${{ needs.version.outputs.version }}     # however the workflow names the version
        path: stats-desktop
    - uses: dtolnay/rust-toolchain@stable
      with:
        targets: ${{ matrix.platform == 'macos-x64' && 'x86_64-apple-darwin' || '' }}
    - name: Libraries the window is built against
      if: runner.os == 'Linux'
      run: sudo apt-get update && sudo apt-get install -y libxkbcommon-dev libwayland-dev libgl1-mesa-dev libx11-dev libxcursor-dev libxrandr-dev libxi-dev desktop-file-utils
    - name: Package
      shell: bash
      working-directory: stats-desktop
      run: packaging/package.sh "${{ needs.version.outputs.version }}" "${{ matrix.platform }}"
    - uses: actions/upload-artifact@v4
      with:
        name: stats-desktop-${{ matrix.platform }}
        path: stats-desktop/dist/*
```

`.github/workflows/ci.yml` here runs the same script on every push, for Windows, Linux and Apple silicon,
after the formatting check, clippy and the tests, and keeps what it made, and the painted screens, on the
run's page for two weeks.

## Licence

GPL-3.0, as the rest of Noctorium. See `LICENSE`.
