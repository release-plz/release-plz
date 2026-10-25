# Installation

Make sure you have `git` installed when running `release-plz`.

`release-plz` is a rust binary that can be installed in different ways.

## Download prebuilt binary

The latest release is on [GitHub](https://github.com/release-plz/release-plz/releases/latest).

## Cargo

* Install the rust toolchain in order to have cargo installed by following
  [this](https://www.rust-lang.org/tools/install) guide.
* Run `cargo install --locked release-plz`.

## Docker

Run `docker pull marcoieni/release-plz`.

## Arch Linux

`release-plz` can be installed from the
[community repository](https://archlinux.org/packages/extra/x86_64/release-plz/)
using [pacman](https://wiki.archlinux.org/title/Pacman).

* `pacman -S release-plz`

There is also a VCS package available in the AUR
and it can be installed with an [AUR helper](https://wiki.archlinux.org/title/AUR_helpers).
For example:

* `paru -S release-plz-git`

## Alpine Linux

`release-plz` is available for [Alpine Edge](https://pkgs.alpinelinux.org/packages?name=release-plz&branch=edge).

It can be installed via [apk](https://wiki.alpinelinux.org/wiki/Alpine_Package_Keeper) after
enabling the [testing repository](https://wiki.alpinelinux.org/wiki/Repositories).

* `apk add release-plz`
