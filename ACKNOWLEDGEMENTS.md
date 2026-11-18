# Acknowledgements and provenance

Tsumugi is independently maintained by the Tsumugi contributors. It is not an
official release of, or affiliated with, the projects acknowledged here.

## Adroit

A substantial part of Tsumugi's initial implementation is derived from
[Adroit](https://github.com/adroit-app/Adroit), by Alisue and contributors.
This includes the workspace model, terminal and work hosts, agent adapters,
Neovim integration, UI components, tests and design documentation. This is
code reuse under its MIT license, rather than inspiration alone.

The starting revision was
[`229d8c74408bbd39d3d4b9cd141b2949e4bf084c`](https://github.com/adroit-app/Adroit/tree/229d8c74408bbd39d3d4b9cd141b2949e4bf084c),
retrieved on 2026-11-16. The original copyright and MIT permission notice are
preserved in [LICENSE](LICENSE), along with the copyright for Tsumugi changes.
The original authorship remains credited here and in `LICENSE`. Tsumugi's
public history begins with its own product snapshot; the linked source
revisions preserve the provenance of the reused implementation.

Updates through [`96500c7`](https://github.com/adroit-app/Adroit/commit/96500c7)
were integrated on 2026-11-18 before creating that snapshot.

Tsumugi has its own name, artwork, package and command names, integrations,
storage namespace, Linux desktop behavior and release configuration. Its
maintainers are responsible for its changes and support. Please file Tsumugi
issues in the Tsumugi repository.

## Libraries and assets

- [felis](https://github.com/felis-terminal/felis) provides the terminal engine
  and persistent terminal sessions.
- [Neovim](https://neovim.io/) provides the prompt, console reader and file editor.
  A bundled Neovim carries its own license and runtime notices.
- [Dioxus](https://dioxuslabs.com/) and WebKitGTK provide the Linux desktop UI.
- [Arto](https://github.com/arto-app/Arto) components provide Markdown rendering;
  frontend package notices are under `assets/arto-frontend/`.
- JetBrains Mono, Nerd Fonts, Tabler Icons, vim-nerdfont and vim-glyph-palette
  supply fonts and file/action icons. Their licenses remain with their assets.
- Solarized colour values are by Ethan Schoonover, used under the MIT license.
  The original notice is in `crates/host/themes/LICENSES/solarized`.
- The bundled console themes retain their authors' notices under
  `crates/host/themes/LICENSES/`.

[THIRD-PARTY-LICENSES.txt](THIRD-PARTY-LICENSES.txt) records the Rust dependency
licenses. Distribution scripts include these notices and the separate asset
licenses. Renaming the application does not change the ownership or licensing
of these components.

## Rebuilding modified versions

Source distributions and build instructions allow users to rebuild Tsumugi
with modified dependencies, including the LGPL-licensed `nvim-rs`. See
[Building and releases](docs/building.md). Tsumugi's license permits rebuilding
and redistributing modifications; release archives include the applicable
license notices.
