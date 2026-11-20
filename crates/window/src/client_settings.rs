//! Display and input preferences belonging to this computer, across
//! environments, and the appearance it writes into each environment's work
//! settings.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tsumugi_core::config::Config;
use tsumugi_core::details::DetailsLayout;
use tsumugi_core::settings::{Appearance, ConsoleSettings, Settings};
use tsumugi_model::EnvironmentId;
use tsumugi_protocol::{ConfigFile, ConfigWork, FileText};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Preferences {
    auto_scale: bool,
    zoom: f64,
    font_family: String,
    font_size: f64,
    font_weight: u16,
    prompt_min_rows: usize,
    prompt_max_rows: usize,
    notify_waiting: bool,
    console: ConsoleSettings,
    appearance: Appearance,
    light_theme: String,
    dark_theme: String,
    #[serde(default)]
    pub(crate) panel: DetailsLayout,
}

/// The appearance and its themes, which every environment's work host is
/// given to choose the theme by.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Looks {
    appearance: Appearance,
    light_theme: String,
    dark_theme: String,
}

impl Looks {
    fn of(settings: &Settings) -> Self {
        Self {
            appearance: settings.appearance,
            light_theme: settings.light_theme.clone(),
            dark_theme: settings.dark_theme.clone(),
        }
    }

    fn apply(&self, settings: &mut Settings) {
        settings.appearance = self.appearance;
        settings.light_theme.clone_from(&self.light_theme);
        settings.dark_theme.clone_from(&self.dark_theme);
    }
}

impl Preferences {
    pub fn of(settings: &Settings) -> Self {
        Self {
            auto_scale: settings.auto_scale,
            zoom: settings.zoom,
            font_family: settings.font_family.clone(),
            font_size: settings.font_size,
            font_weight: settings.font_weight,
            prompt_min_rows: settings.prompt_min_rows,
            prompt_max_rows: settings.prompt_max_rows,
            notify_waiting: settings.notify_waiting,
            console: settings.console.clone(),
            appearance: settings.appearance,
            light_theme: settings.light_theme.clone(),
            dark_theme: settings.dark_theme.clone(),
            panel: settings.panel.clone(),
        }
    }
    pub fn apply(&self, settings: &mut Settings) {
        settings.auto_scale = self.auto_scale;
        settings.zoom = self.zoom;
        settings.font_family.clone_from(&self.font_family);
        settings.font_size = self.font_size;
        settings.font_weight = self.font_weight;
        settings.prompt_min_rows = self.prompt_min_rows;
        settings.prompt_max_rows = self.prompt_max_rows;
        settings.notify_waiting = self.notify_waiting;
        settings.console = self.console.clone();
        settings.panel = self.panel.clone();
        self.looks().apply(settings);
    }

    fn looks(&self) -> Looks {
        Looks {
            appearance: self.appearance,
            light_theme: self.light_theme.clone(),
            dark_theme: self.dark_theme.clone(),
        }
    }

    /// `text` as `window.toml` holds it, the preferences it does not name,
    /// as one written before they were kept there does not, taken from
    /// `earlier`.
    fn read(text: &str, earlier: impl FnOnce() -> Result<Self, String>) -> Result<Self, String> {
        let mut table: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
        // The machine's meters were chosen here before the details panel
        // placed each of them; a file that still names them is read without.
        table.remove("usage");
        // This flag is new and local to the display. Upgrading a complete
        // window file must not require rereading unrelated legacy settings.
        table
            .entry("auto_scale")
            .or_insert(toml::Value::Boolean(true));
        tsumugi_core::settings::remove_retired_console_settings(&mut table);
        // A panel this version cannot read, as one another version wrote,
        // leaves Tsumugi's in its place rather than every preference unread
        // and unwritable.
        if let Some(Err(error)) = table
            .get("panel")
            .map(|p| p.clone().try_into::<DetailsLayout>())
        {
            tracing::debug!(%error, "the details panel in window.toml cannot be read; Tsumugi's is shown");
            let shipped =
                toml::Value::try_from(DetailsLayout::default()).map_err(|e| e.to_string())?;
            table.insert("panel".into(), shipped);
        }
        let all =
            toml::Table::try_from(Self::of(&Settings::default())).map_err(|e| e.to_string())?;
        if all.keys().any(|key| !table.contains_key(key)) {
            let earlier = toml::Table::try_from(earlier()?).map_err(|e| e.to_string())?;
            for (key, value) in earlier {
                table.entry(key).or_insert(value);
            }
        }
        table.try_into().map_err(|e: toml::de::Error| e.to_string())
    }
}

fn path() -> std::path::PathBuf {
    crate::paths::config_dir().join("window.toml")
}

/// The preferences the work settings in this computer's configuration folder
/// hold, which earlier versions kept them in.
fn earlier() -> Result<Preferences, String> {
    let legacy = crate::paths::config_dir().join("settings.toml");
    let text = tsumugi_connect::read_config(&legacy, 1024 * 1024)
        .map_err(|e| e.to_string())?
        .map_or(FileText::Missing, |text| FileText::Read { text });
    Settings::read(&ConfigFile { path: legacy, text }).map(|s| Preferences::of(&s))
}

pub(crate) fn load() -> Result<Preferences, String> {
    let path = path();
    match tsumugi_connect::read_config(&path, 65536).map_err(|e| e.to_string())? {
        Some(text) => {
            Preferences::read(&text, earlier).map_err(|e| format!("{}: {e}", path.display()))
        }
        None => earlier(),
    }
}

pub(crate) fn save(settings: &Settings) -> Result<(), String> {
    load()?;
    tsumugi_connect::environments::write(&path(), &Preferences::of(&settings.clone().sane()))
}

/// The preferences in force: this computer's, once it is bound to an
/// environment and they can be read.
pub(crate) fn chosen() -> Option<Preferences> {
    tsumugi_connect::environments::active()
        .ok()
        .flatten()
        .and(load().ok())
}

pub(crate) fn apply(settings: &mut Settings) {
    if let Some(preferences) = chosen() {
        preferences.apply(settings);
    }
}

/// The work settings `work` holds with the appearance and themes of
/// `preferences`, to be written over the text they hold now: nothing when
/// they name these already, or cannot be read, so as not to be overwritten.
pub(crate) fn appearance_written(
    work: &ConfigWork,
    preferences: &Preferences,
) -> Result<Option<(String, FileText)>, String> {
    let Some(file) = &work.settings else {
        return Ok(None);
    };
    let config = Config::of_work(work);
    if config.settings_error.is_some() || Looks::of(&config.settings) == preferences.looks() {
        return Ok(None);
    }
    let mut settings = config.settings;
    preferences.looks().apply(&mut settings);
    let text = toml::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    Ok(Some((text, file.text.clone())))
}

/// The appearance last asked to be written into each environment's work
/// settings, so the same is asked once while the window runs: not again after
/// a save refused or failed, nor after another client wrote its own, which
/// would have the two write over each other for ever.
#[derive(Default)]
pub(crate) struct Written(HashMap<EnvironmentId, Looks>);

impl Written {
    /// Whether to ask `environment`'s host to write `preferences`'
    /// appearance; once asked, not again for the same.
    pub(crate) fn due(&mut self, environment: &EnvironmentId, preferences: &Preferences) -> bool {
        let asked = preferences.looks();
        if self.0.get(environment) == Some(&asked) {
            return false;
        }
        self.0.insert(environment.clone(), asked);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retired_external_terminal_preferences_leave_the_other_console_preferences_usable() {
        let mut file = toml::Table::try_from(Preferences::of(&Settings::default())).unwrap();
        let console = file.get_mut("console").unwrap().as_table_mut().unwrap();
        console.insert("display".into(), "kitty".into());
        console.insert(
            "external_command".into(),
            toml::Value::Array(vec!["kitty".into()]),
        );
        console.insert("cursor_blink".into(), true.into());
        let read = Preferences::read(&toml::to_string(&file).unwrap(), || {
            panic!("all preferences saved")
        })
        .unwrap();
        assert!(read.console.cursor_blink);
        let saved = toml::Table::try_from(&read).unwrap();
        let console = saved.get("console").unwrap().as_table().unwrap();
        assert!(!console.contains_key("display"));
        assert!(!console.contains_key("external_command"));
    }

    fn settings_file(text: &str) -> ConfigFile {
        ConfigFile {
            path: "/c/settings.toml".into(),
            text: FileText::Read { text: text.into() },
        }
    }

    #[test]
    fn client_preferences_do_not_change_agents_or_working_paths() {
        let first = Settings {
            font_family: "Client font".into(),
            zoom: 1.25,
            ..Settings::default()
        };
        let preferences = Preferences::of(&first);
        let mut work = Settings {
            scratch_dir: "/remote/work".into(),
            ..Settings::default()
        };
        let agents = work.profiles.clone();
        preferences.apply(&mut work);
        assert_eq!(work.zoom, 1.25);
        assert_eq!(work.font_family, "Client font");
        assert_eq!(work.scratch_dir, "/remote/work");
        assert_eq!(work.profiles, agents);
    }

    fn mine() -> Settings {
        Settings {
            appearance: Appearance::Dark,
            light_theme: "Dayfox".into(),
            dark_theme: "Nord".into(),
            panel: DetailsLayout {
                github: Some(tsumugi_core::details::Surface {
                    foot: vec![tsumugi_core::details::Placed::new(
                        tsumugi_core::details::Component::Memory,
                    )],
                    ..tsumugi_core::details::Surface::github()
                }),
                ..DetailsLayout::default()
            },
            ..Settings::default()
        }
    }

    #[test]
    fn the_appearance_and_the_panel_shown_are_the_client_s_over_any_environment_s() {
        let preferences = Preferences::of(&mine());
        let mut work = Settings {
            keep_awake: true,
            ..Settings::default()
        };
        preferences.apply(&mut work);
        assert_eq!(work.appearance, Appearance::Dark);
        assert_eq!(work.light_theme, "Dayfox");
        assert_eq!(work.dark_theme, "Nord");
        assert_eq!(work.panel, mine().panel);
        assert!(work.keep_awake);
    }

    #[test]
    fn the_preferences_are_written_and_read_back_as_they_were() {
        let preferences = Preferences::of(&mine());
        let text = toml::to_string_pretty(&preferences).unwrap();
        let read = Preferences::read(&text, || panic!("names every preference"));
        assert_eq!(read, Ok(preferences));
    }

    #[test]
    fn preferences_written_before_some_were_kept_take_those_from_the_earlier_settings() {
        let text = "zoom = 1.5\nfont_family = \"Mine\"\nfont_size = 13.0\nfont_weight = 400\n\
                    prompt_min_rows = 2\nprompt_max_rows = 12\nnotify_waiting = true\n[console]\n";
        let read = Preferences::read(text, || Ok(Preferences::of(&mine()))).unwrap();
        let mut settings = Settings::default();
        read.apply(&mut settings);
        assert_eq!(settings.zoom, 1.5, "what the file names is kept");
        assert_eq!(settings.font_family, "Mine");
        assert_eq!(
            settings.dark_theme, "Nord",
            "the rest is the earlier settings'"
        );
        assert_eq!(settings.appearance, Appearance::Dark);
    }

    #[test]
    fn a_panel_this_version_cannot_read_leaves_tsumugi_s_and_the_other_preferences_read() {
        let mut text = toml::to_string_pretty(&Preferences::of(&Settings::default())).unwrap();
        text.push_str("[panel.session]\nfoot = [\"machine\"]\n");
        let read = Preferences::read(&text, || panic!("names every preference")).unwrap();
        assert_eq!(read.panel, DetailsLayout::default());
        assert_eq!(read, Preferences::of(&Settings::default()));
    }

    #[test]
    fn the_machine_s_meters_an_earlier_version_chose_here_are_left_unread() {
        let mut text = toml::to_string_pretty(&Preferences::of(&mine())).unwrap();
        text.push_str("[usage]\ncpu = false\n");
        let read = Preferences::read(&text, || panic!("names every preference"));
        assert_eq!(read, Ok(Preferences::of(&mine())));
    }

    fn work(text: &str) -> ConfigWork {
        ConfigWork {
            settings: Some(settings_file(text)),
            ..ConfigWork::default()
        }
    }

    #[test]
    fn an_environment_naming_another_appearance_is_written_this_one_with_its_other_settings_kept() {
        let host = work("keep_awake = true\nzoom = 1.5\n[usage]\ncpu = false\n");
        let (text, expected) = appearance_written(&host, &Preferences::of(&mine()))
            .unwrap()
            .expect("the host names another appearance");
        assert_eq!(expected, host.settings.as_ref().unwrap().text);
        let written = Settings::read(&settings_file(&text)).unwrap();
        assert_eq!(written.appearance, Appearance::Dark);
        assert_eq!(written.light_theme, "Dayfox");
        assert_eq!(written.dark_theme, "Nord");
        assert!(written.keep_awake);
        assert_eq!(
            written.zoom, 1.5,
            "only the appearance is the client's to write"
        );
        assert!(!written.usage.cpu);
    }

    #[test]
    fn an_environment_already_naming_this_appearance_or_whose_settings_cannot_be_read_is_left_alone()
     {
        let preferences = Preferences::of(&mine());
        let same = work(
            "appearance = \"dark\"\nlight_theme = \"Dayfox\"\ndark_theme = \"Nord\"\nzoom = 2.0\n",
        );
        assert_eq!(appearance_written(&same, &preferences), Ok(None));
        assert_eq!(appearance_written(&work("zoom = "), &preferences), Ok(None));
        assert_eq!(
            appearance_written(&ConfigWork::default(), &preferences),
            Ok(None)
        );
    }

    #[test]
    fn the_appearance_is_asked_written_once_until_the_client_s_choice_changes() {
        let here = EnvironmentId::new("personal");
        let mut written = Written::default();
        let preferences = Preferences::of(&mine());
        assert!(written.due(&here, &preferences));
        assert!(
            !written.due(&here, &preferences),
            "refused, or changed back by another client, not asked again"
        );
        let zoomed = Preferences::of(&Settings {
            zoom: 2.0,
            ..mine()
        });
        assert!(!written.due(&here, &zoomed), "nothing written changed");
        let light = Preferences::of(&Settings {
            appearance: Appearance::Light,
            ..mine()
        });
        assert!(written.due(&here, &light), "the client chose another");
        assert!(
            written.due(&EnvironmentId::new("other"), &light),
            "each environment is asked on its own"
        );
    }
    #[test]
    fn automatic_scaling_is_a_local_preference_and_survives_saving_and_migration() {
        let earlier = || Ok(Preferences::of(&Settings::default()));
        let old = Preferences::read("zoom = 1.25", earlier).unwrap();
        assert!(old.auto_scale);
        let chosen = Preferences::of(&Settings {
            auto_scale: false,
            ..Settings::default()
        });
        let saved = toml::to_string(&chosen).unwrap();
        let read = Preferences::read(&saved, || panic!("all preferences were saved")).unwrap();
        let mut remote = Settings::default();
        read.apply(&mut remote);
        assert!(!remote.auto_scale);
    }
    #[test]
    fn an_existing_window_file_gets_the_new_local_flag_without_loading_legacy_work_settings() {
        let mut old = toml::Table::try_from(Preferences::of(&Settings::default())).unwrap();
        old.remove("auto_scale");
        let loaded = Preferences::read(&toml::to_string(&old).unwrap(), || {
            panic!("a complete older window file must not depend on legacy work settings")
        })
        .unwrap();
        assert!(loaded.auto_scale);
    }
}
