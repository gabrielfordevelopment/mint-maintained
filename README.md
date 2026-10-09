# MINT Maintained

An independently maintained MINT mod manager for Deep Rock Galactic, maintained by
[Gabriel](https://github.com/gabrielfordevelopment).

This project continues [Trumank's MINT](https://github.com/trumank/mint) and
[Wasserkleber's mintfixed](https://github.com/Wasserkleber/mintfixed). Credit for the
original application and inherited fixes belongs to their authors and contributors.
The original license and commit history are preserved.

Development, issues, and releases are managed in this repository. The GitHub fork
relationship records the project's origin; releases do not depend on upstream approval.

This version includes grouped-profile sorting fixes, missing-metadata regression tests,
and compact UI improvements with Material Symbols SVG icons. It also retains these
fixes from mintfixed:
- Fixed HTTP 403 errors (mod lookup) by removing the deprecated `visible` filter from mod.io API requests.
- Fixed HTTP 403 errors by replacing direct mod file metadata requests (`GET /mods/{id}/files/{file_id}`) with filtered list queries, restoring mod downloads.
- Limits the mission selector/server browser mod list to 100 entries, preventing hosting/invites from breaking when the mission selector mod list string becomes too large.
- Updated Trumans Repaker to make mods that need oodle compression work again automatically without needing to manually add a DLL (for example: https://mod.io/g/drg/m/missions-hud)

## Overview

3rd party mod integration tool for Deep Rock Galactic to download and integrate mods completely
externally of the game. This enables more stable mod usage as well as offline mod usage. Works for
both Steam and Microsoft Store versions.

<img alt="Graphical User Interface" src="https://github.com/trumank/mint/assets/1144160/0305419f-a2af-4349-9d63-12e19d97102f">

Mods are added via URL to a .pak or .zip containing a .pak. Mods can also be pulled from mod.io.
Examples:

 - `C:\Path\To\Local\Mod.zip`
 - `https://example.org/some-online-mod-repository/public-mod.pak`
 - `https://mod.io/g/drg/m/sandbox-utilities`

Mods from mod.io will require an OAuth token which can be obtained from <https://mod.io/me/access>
when prompted.

Most mods work just as if they were loaded via the official integration, but there are still some
behavioural differences. If a mod is crashing or otherwise behaving differently than when using the
official integration, *please* create an
[issue](https://github.com/gabrielfordevelopment/mint-maintained/issues/new) so it can be addressed.

The [original MINT user guide](https://github.com/trumank/mint/wiki) is useful background;
some details and the inherited screenshots below may differ from this maintained version.

## Usage

This section assumes that you are on Windows and is using the steam version of DRG, working with
either local `.pak`s or mod.io mods.

Download a build from this repository's [releases](https://github.com/gabrielfordevelopment/mint-maintained/releases)
compatible with your architecture. For windows, this will be the
`mint-x86_64-pc-windows-msvc.zip`. Extract this to anywhere you'd like to keep the
executable.

The first MINT Maintained release has not been published yet. Until then, build from
source using the instructions below. Update checks and downloads target this repository.

Then, we'll need to perform some first-time setup.

### First Time Setup

We need to provide the tool with the path to `FSD-WindowsNoEditor.pak` and a mod.io OAuth token if
you want to use mod.io mods. These can be configured in the settings menu (cogwheel located in the
bottom toolbar).

<img alt="Settings menu" src="https://github.com/trumank/mint/assets/1144160/b009a74c-b13a-4b84-95f9-4c59c6debb62">

#### Locating the DRG `FSD-WindowsNoEditor.pak`

If the tool fails to detect your DRG installation, then you can manually browse to add the path to
`FSD-WindowsNoEditor.pak`.

This file is located under the `FSD` folder inside your DRG installation directory, e.g.

```
E:\SteamLibrary\steamapps\common\Deep Rock Galactic\FSD\FSD-WindowsNoEditor.pak
```

#### Adding a mod.io OAuth Token

Inside the settings menu, there is a modio setting (cogwheel). If you click on that, it will prompt
for an mod.io OAuth token.

To generate a mod.io OAuth token, you'll need to visit <https://mod.io/me/access>. You'll need to
accept the API terms and conditions.

<img alt="mod.io Access page" src="https://github.com/trumank/mint/assets/1144160/2aeb6135-71c2-4c3c-8979-49e84b276bed">

Then, you'll need to add a new client under OAuth Access, call it e.g. "DRG Mod Integration".

For that client, create a new token named e.g. "modio-access" with Read-only scope. Copy the token
into the integration tool's prompt.

### Adding Mods

After these steps, you can now add local mods or mod.io mods.

#### Adding mod.io mods

Copy the URL to the mod into the "Add mods..." field and hit enter.

You can obtain a list of your subscribed mods list using the "Copy Mod URLs"
button via [A Better Modding Menu](https://mod.io/g/drg/m/a-better-modding-menu)
in game:

![Copy Mod URLs](https://github.com/trumank/mint/assets/1144160/375f441f-4762-4549-a241-1b54ed391b2f)

#### Adding a local mod

You can either drag and drop a local `.pak` file on to the tool window, or add the path to the
local `.pak` in the same "Add mods..." field.

### Sorting mods

Sorting changes the displayed order only; selecting Manual restores the stored order.
Groups stay in place. Mods are sorted within each group and within each contiguous
run of individual mods between groups.

### Updating Cache

The versioned mod.io mods are *cached*. If you want to update to the latest version of your mods,
you'll need to press the "Update cache" button.

### Installing/uninstalling mods

Once you are happy with your mod profile, you can install the mods by pressing the "Install mods"
button, and uninstall them with the "Uninstall mods" button. **This must be done while the game is
closed.**

## Using integrated mod support again

If you want to go back to the integrated mod support again, you must uninstall the mods installed by
mint. Then, launch the game normally.

## Development

Install Rust through rustup and the native build tools for your platform. This project
uses the nightly toolchain specified in `rust-toolchain.toml`.

On Windows, use Visual Studio 2022 Build Tools with Desktop development with C++ and
a Windows SDK, and run Cargo from its x64 Developer PowerShell. On Linux, install
`libgtk-3-dev`, `gcc-mingw-w64`, and `pkg-config`.

```sh
cargo fmt -- --check
cargo test --locked -p mint -p mint_lib
cargo clippy --locked --all-features --all-targets -- -D warnings
cargo build --locked
cargo run --locked -- --appdata ../mint-maintained-dev-data
```

Using `--appdata` keeps development profiles separate from regular app data. Existing
MINT data paths and the `mint` executable name are retained for compatibility.

Create feature and fix branches from this repository's `develop` branch and target
pull requests to `develop` here. The names `dev` and `development` also refer to
`develop`; they are not separate branches. `master` remains the release branch.
The previously submitted upstream PR branch is retained separately. Agent guidance
is in [AGENTS.md](AGENTS.md), including the repository's Git workflow skill.

## Releases

The Release workflow builds Windows and Linux ZIP archives. A manual run produces
downloadable workflow artifacts without publishing a release. For a public release,
prepare the workspace version and changelog on a task branch targeting `develop`,
then promote the tested changes through a release PR from `develop` to `master`.
Push a matching `vX.Y.Z` tag on the tested release commit on `master`. Tag builds
verify that the commit is part of `master` and that the tag matches the workspace
version, then create a draft GitHub release after both platform builds succeed;
review the archives and publish the draft explicitly. Use a version above `0.2.10`
for the first maintained release so existing version comparisons recognize it.

Archives retain the `mint-<target>.zip` names expected by the updater and include the
project license and third-party notices. No release is published by a normal branch push.
