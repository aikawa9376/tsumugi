//! The page about tsumugi itself: which build this is, where its files are
//! kept, and where its source and its issues are.

use dioxus::prelude::*;

use tsumugi_model::paths::tilde;

use super::{KEYS_STARTER, open_with, run_open};
use crate::build::build;
use crate::components::{Button, SettingGroup, SettingRow};
use crate::feed::use_config;
use crate::icons::{Icon, IconView};
use crate::link::Link;
use crate::paths::home;

const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

const ICON: &str = include_str!("../../../../assets/logo/tsumugi-icon.svg");

#[component]
pub(super) fn AboutPane() -> Element {
    let remote = use_context::<Link>().remote_status().is_some();
    let config = use_config();
    let files = {
        let config = config.read();
        let settings = config.files().settings.as_ref();
        // The work settings are on the environment's work machine, the key
        // bindings on this computer.
        [
            (
                "Settings",
                settings.map(|f| f.path.clone()).unwrap_or_default(),
                "",
                remote,
            ),
            (
                "Key bindings",
                crate::client_keys::path(),
                KEYS_STARTER,
                false,
            ),
        ]
    };
    let version = build();
    // The Neovim the prompt and the console's reader run, which a report of
    // a problem with them needs as much as tsumugi's own version.
    let neovim = use_resource(tsumugi_nvim::version);
    let neovim = match &*neovim.read() {
        Some(Some(v)) => format!("Neovim {v}"),
        Some(None) => "Neovim cannot be run".into(),
        None => "Neovim …".into(),
    };
    rsx! {
        div { class: "about",
            div { class: "about-icon", dangerous_inner_html: ICON }
            h1 { class: "about-name", "Tsumugi" }
            p { class: "about-version", "Version {version}" }
            p { class: "about-version", "{neovim}" }
            p { class: "about-tagline",
                "An Agent Workspace for people who live in Vim: drive any coding agent in its own terminal interface, by hand and the way you edit."
            }
        }
        SettingGroup { title: "Files",
            for (name, path, starter, remote) in files {
                SettingRow { key: "{name}", label: name, hint: if remote { path.display().to_string() } else { tilde(&path, &home()) },
                    Button {
                        disabled: remote,
                        title: "Open in the text editor",
                        onclick: {
                            let path = path.clone();
                            move |_| if !remote { open_with("-t", &path, starter); }
                        },
                        IconView { icon: Icon::FilePencil, size: 14 }
                        "Open"
                    }
                    Button {
                        disabled: remote,
                        onclick: move |_| if !remote { open_with("-R", &path, starter); },
                        IconView { icon: Icon::FolderOpen, size: 14 }
                        "Show in file manager"
                    }
                }
            }
        }
        div { class: "about-links",
            button { class: "about-link", onclick: move |_| run_open(REPOSITORY),
                IconView { icon: Icon::BrandGithub, size: 18 }
                span { class: "about-link-text",
                    span { "Source code" }
                    span { class: "settings-hint", "Tsumugi on GitHub" }
                }
            }
            button { class: "about-link", onclick: move |_| run_open(format!("{REPOSITORY}/issues")),
                IconView { icon: Icon::Bug, size: 18 }
                span { class: "about-link-text",
                    span { "Issues and ideas" }
                    span { class: "settings-hint", "Report a problem or suggest something for Tsumugi" }
                }
            }
        }
        button { class: "about-link", onclick: move |_| run_open(format!("{REPOSITORY}/blob/main/ACKNOWLEDGEMENTS.md")),
            "Credits and open source licenses"
        }
        p { class: "about-credit", "Tsumugi contributors · MIT License · An independent hobby project" }
    }
}
