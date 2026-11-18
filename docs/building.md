# Building and releases

Tsumugi is developed primarily on Linux. `README.md` lists desktop libraries
and runtime helpers. `nix develop` provides the development tools; alternatively
install Rust, just, Neovim, Node.js, jj and Ghostscript alongside those libraries.
The Rust toolchain and bundled Neovim are pinned in the repository.

Run `just ci` before submitting a change. It builds a version without the
desktop runtime last, so rebuild `cargo build --locked -p tsumugi` before
launching `target/debug/tsumugi` directly. `just run` selects the desktop build.

## Packaging

`just bundle-linux VERSION` builds a Linux archive with its installer, desktop
icon, full license notices and a source-code reference. Build on the oldest
Linux distribution you intend to support: the executable links system GTK,
WebKitGTK and other libraries and is not a universal static binary.

On macOS, `just bundle VERSION` builds the application and disk image. A new
maintainer must supply their own signing identity for signed distribution;
ad hoc signatures are used locally. Windows remains a source build.

A published GitHub Release builds artifacts in this repository, attaches their
checksums and includes a source archive with vendored Cargo dependencies.
The release version is stamped from the release tag; no external Homebrew tap,
upstream signing account or private build cache is required.

## Rebuilding and relinking

The application is MIT licensed; dependency licenses are listed separately.
To rebuild with a modified LGPL `nvim-rs`, unpack the matching source archive,
modify its vendored source and run `cargo build --release --locked -p tsumugi`.
The included `.cargo/config.toml` selects the vendored crates. The complete
application source, lockfile and build scripts are included so a modified
library can be relinked into the application. System libraries and the Rust
toolchain are prerequisites listed above. A modified macOS app can be signed
ad hoc using the normal bundle script.

## Repository identity

Application commands, internal packages and advanced environment variables use
`tsumugi`, `tsumugi-*` and `TSUMUGI_*`. Old pre-release terminal tags can still
be read so existing local terminal resources remain discoverable. Current
writes use the Tsumugi namespace. Source attribution lives in
`ACKNOWLEDGEMENTS.md` and the original notices remain in `LICENSE`.
