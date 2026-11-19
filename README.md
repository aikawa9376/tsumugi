<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logo/tsumugi-logo-horizontal-light.svg">
    <img src="assets/logo/tsumugi-logo-horizontal.svg" alt="Tsumugi" width="360">
  </picture>
</p>

<p align="center">Coding agents, terminals and your Neovim. One place to weave the work together.</p>

Tsumugi（つむぎ）is a cross-platform desktop workspace for people who work with
coding agents and live in Vim. Keep agents beside your files and shells,
write prompts in a real Neovim, and move between them with familiar keys.

This is an independent hobby project. Expect changes, rough edges and
limited platform coverage; issues and small, focused contributions are welcome.

[Japanese guide](docs/guide.ja.md) · [Key bindings (日本語)](docs/keybindings.ja.md) · [Concept](docs/concept.md) ·
[Specifications](docs/spec/README.md) · [Building and releases](docs/building.md)

## Work in your own tools

- Run Claude Code, Codex, Pi and OpenCode in their own terminal interfaces.
- Write prompts in embedded Neovim and edit files with your normal Neovim setup.
- Keep projects, worktrees, sessions and shells together, with tabs and splits.
- Read changes, conversation history and session activity beside the work.
- Keep Tree and Agents together in the sidebar, with more room for the section in focus.
- Queue local issues from a `+` menu for the default agent to work through.
- Close a window and return to the sessions still running in the background.
- Use an integrated Linux header, system clipboard, notifications and display scaling.

You install and authenticate the agents you want to use. Tsumugi does not
provide model access or replace their account settings.

## Run on Linux

Install Rust (the repository pins its toolchain), Neovim, Git and at least one
supported agent. Pi and OpenCode integrations require their supported current
CLI versions; OpenCode requires 2.1 or later.

```sh
# Debian / Ubuntu
sudo apt install build-essential pkg-config libgtk-3-dev libwebkit2gtk-4.1-dev \
  libsoup-3.0-dev libssl-dev libxdo-dev wl-clipboard xclip xdg-utils \
  libnotify-bin ghostscript

# Arch Linux
sudo pacman -S --needed base-devel pkgconf gtk3 webkit2gtk-4.1 libsoup3 \
  openssl xdotool wl-clipboard xclip xdg-utils libnotify ghostscript

cargo run --locked -p tsumugi
```

Choose a directory with `Ctrl+Shift+p`, select an agent profile, write a prompt
and press `Ctrl+Enter`. `F1` shows the keys. Settings → Agents manages profiles;
Settings → Programs chooses your shell and file editor.

Configuration and state use the `tsumugi` directory beneath the corresponding
XDG roots. Advanced controls use `TSUMUGI_*`. `--instance playground` separates
workspace state while preserving your shell and editor environment.

## Install

```sh
cargo build --release --locked -p tsumugi
assets/linux/install.sh
```

The installer places the app, desktop launcher, icon and license notices under
`~/.local`. It also accepts `--prefix PATH` and `--binary PATH` and requires
Python 3. Background services for paired clients are enabled separately with
`tsumugi remote enable`.

## Run on macOS

Apple Silicon and Intel Macs have separate disk images in
[GitHub Releases](https://github.com/aikawa9376/tsumugi/releases): choose
`aarch64` for Apple Silicon or `x86_64` for Intel, open the image and drag
Tsumugi to Applications. The app carries Neovim; install and authenticate the
agents you want to use separately.

For a source build, install the Xcode Command Line Tools, Rust, Neovim and Git,
then run the same `cargo run --locked -p tsumugi` command as on Linux. macOS
uses its system webview and does not need GTK or WebKitGTK. With the
[development tools](docs/building.md) installed, build a standalone app with:

```sh
just build-macos
open target/macos/Tsumugi.app
```

Move the built app to `/Applications` to keep it. Builds without a signing
identity use ad hoc signing and are not notarized; macOS may require you to allow
the app in System Settings → Privacy & Security before opening it. macOS keeps
the native menu bar and window controls; `F1` shows the active keys.

## Platform coverage

Linux remains the primary development environment. CI runs the quality gate
on Linux, macOS (Apple Silicon and Intel), and Windows. The release workflow
packages Linux archives and both Mac disk images; Windows remains a source
build. Desktop interaction, input methods and permissions still need testing
on the actual OS. Agent availability, authentication and remote machines
require their own setup.

## Development

```sh
just run
just ci
```

The [build guide](docs/building.md) lists the tools and release process.
`just ci` checks formatting, Clippy, workspace tests with the pinned Neovim and
the build without the desktop runtime. Keep contributions small and include a
regression test for behavior changes.

## License and credits

Tsumugi is distributed under the [MIT license](LICENSE). Its implementation
includes open source code and assets with their own notices; see
[acknowledgements and provenance](ACKNOWLEDGEMENTS.md) and
[third-party licenses](THIRD-PARTY-LICENSES.txt). Those notices accompany
packaged distributions.
