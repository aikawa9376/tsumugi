<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logo/tsumugi-logo-horizontal-light.svg">
    <img src="assets/logo/tsumugi-logo-horizontal.svg" alt="Tsumugi" width="360">
  </picture>
</p>

<p align="center">Coding agents, terminals and your Neovim. One place to weave the work together.</p>

Tsumugi（つむぎ）is a Linux-first desktop workspace for people who work with
coding agents and live in Vim. Keep agents beside your files and shells,
write prompts in a real Neovim, and move between them with familiar keys.

This is an independent hobby project. Expect changes, rough edges and
limited platform coverage; issues and small, focused contributions are welcome.

[Japanese guide](docs/guide.ja.md) · [Concept](docs/concept.md) ·
[Specifications](docs/spec/README.md) · [Building and releases](docs/building.md)

## Work in your own tools

- Run Claude Code, Codex, Pi and OpenCode in their own terminal interfaces.
- Write prompts in embedded Neovim and edit files with your normal Neovim setup.
- Keep projects, worktrees, sessions and shells together, with tabs and splits.
- Read changes, conversation history and session activity beside the work.
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

Linux is the primary development platform. macOS and Windows code is retained,
but support on those systems depends on testing and contributions. Agent
availability, authentication and remote machines require their own setup.

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
