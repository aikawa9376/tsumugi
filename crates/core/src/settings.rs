//! The window's settings as `settings.toml` writes them: what each one is,
//! what it is when the file does not say, and what is brought within reason.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub use tsumugi_protocol::ThemeKind;

pub use tsumugi_model::Agent;
pub use tsumugi_model::config::CodexOptions;
pub use tsumugi_model::config::{Launch, SessionOptions, ShellLaunch, UsageSettings};
pub use tsumugi_model::profile::{AgentOptions, AgentProfile, ProfileId};

/// What becomes of the sessions and terminals a terminal host took with it
/// when it went: restarted, crashed or with the machine.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum RestoreSessions {
    /// The window offers to bring them back.
    #[default]
    Ask,
    /// They come back at once.
    Always,
    /// They are let go; Resume Conversation still finds each conversation.
    Never,
}

impl RestoreSessions {
    pub fn name(self) -> &'static str {
        match self {
            RestoreSessions::Ask => "ask",
            RestoreSessions::Always => "always",
            RestoreSessions::Never => "never",
        }
    }
}

/// Which Option keys count as Alt, sending Meta, in a console; the others type
/// what macOS makes of the key with Option, as `Option-2` typing `™`.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum OptionAsAlt {
    #[default]
    Both,
    Left,
    Right,
    None,
}

impl OptionAsAlt {
    pub fn name(self) -> &'static str {
        match self {
            OptionAsAlt::Both => "both",
            OptionAsAlt::Left => "left",
            OptionAsAlt::Right => "right",
            OptionAsAlt::None => "none",
        }
    }
}

/// Which of the light and the dark theme is drawn.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

impl Appearance {
    pub fn name(self) -> &'static str {
        match self {
            Appearance::System => "system",
            Appearance::Light => "light",
            Appearance::Dark => "dark",
        }
    }
}

/// The theme drawn in a light appearance when the settings name none.
pub const LIGHT_THEME: &str = "Solarized Light";

/// The theme drawn in a dark appearance when the settings name none.
pub const DARK_THEME: &str = "Solarized Dark";

/// How much the details panel reads and shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct DetailsSettings {
    /// The changes Log lists.
    pub log_commits: usize,
    /// The lines of a diff shown before it is cut.
    pub diff_lines: usize,
    /// The links of each kind the overview keeps, the newest, up to 1000.
    pub links_kept: usize,
    /// The links of each kind the overview shows before a row that shows them
    /// all.
    pub links_shown: usize,
}

impl Default for DetailsSettings {
    fn default() -> Self {
        Self {
            log_commits: 50,
            diff_lines: 2000,
            links_kept: 100,
            links_shown: 5,
        }
    }
}

/// The display used for focused consoles.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum TerminalRenderer {
    #[default]
    Builtin,
    Vte,
    Kitty,
}

impl TerminalRenderer {
    pub fn name(self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::Vte => "vte",
            Self::Kitty => "kitty",
        }
    }
}

/// How the console is drawn and takes keys.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ConsoleSettings {
    /// Console display: `builtin`, `vte` for Linux terminal tabs with GTK3 VTE,
    /// or `kitty` for Linux session and terminal consoles with kitty and an
    /// X11/XWayland window.
    /// The built-in display is used when the chosen display is unavailable,
    /// while images are shown, and for Neovim GUI terminals.
    pub renderer: TerminalRenderer,
    /// Which Option keys send Alt to what runs there, `both`, `left`, `right` or
    /// `none`, the others typing what macOS makes of the key.
    pub option_as_alt: OptionAsAlt,
    /// Whether the cursor blinks when the program lets it.
    pub cursor_blink: bool,
}

/// Remove retired external-display preferences from work settings, recorded
/// launches and local window preferences without discarding other console keys.
pub fn remove_retired_console_settings(table: &mut toml::Table) {
    if let Some(console) = table.get_mut("console").and_then(toml::Value::as_table_mut) {
        for key in ["display", "external_command"] {
            console.remove(key);
        }
    }
}

// `remote = "Self"` turns the derives into the inherent `Settings::serialize`
// and `Settings::deserialize`, which read and write these fields alone; the
// trait impls below wrap them, so a file and a launch the work host kept are
// read, and checked, the one way, through what `earlier` takes out first.
// Call `<Settings as Deserialize>::deserialize`, never the inherent one.
// `default` stays for the reference, which takes each setting's default
// from it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(remote = "Self", default, deny_unknown_fields)]
#[schemars(rename = "Settings")]
pub struct Settings {
    /// The profile a new session starts from, by id, when it names neither a
    /// profile nor an agent; left out, the first profile. A
    /// session not started yet can be switched to another agent for the
    /// current run of the window.
    pub profile: ProfileId,
    /// The profiles sessions start from, each `[[profiles]]` table one of them
    /// ([settings](settings.md#profiles)). An agent's default profile is the
    /// default profile when that starts the agent, else the agent's first
    /// profile.
    pub profiles: Vec<AgentProfile>,
    /// The profiles that a file written with connections had
    /// share one with a profile before them, and that run in their agent's
    /// own environment since; read to say so, never written.
    #[serde(skip)]
    #[schemars(skip)]
    pub unshared: Vec<ProfileId>,
    /// How sessions opened by sessions run.
    pub delegation: DelegationSettings,
    /// The user's zoom adjustment, multiplied by the display scale when automatic scaling is enabled.
    pub zoom: f64,
    /// Adapt window scale to the current display.
    pub auto_scale: bool,
    /// The fewest lines the prompt shows.
    pub prompt_min_rows: usize,
    /// The most lines the prompt grows to before it scrolls inside.
    pub prompt_max_rows: usize,
    /// The font the console and the prompt are drawn in, as the system names its
    /// family; empty for Tsumugi's own, JetBrainsMono Nerd Font Mono. Tsumugi's own
    /// stands behind any other, so Nerd Font glyphs draw whatever font is chosen,
    /// and a Japanese font behind that. A line is as tall as the font asks, as in
    /// a terminal.
    pub font_family: String,
    /// The font's size, in points, from 8 to 32.
    pub font_size: f64,
    /// The font's weight, from 100 to 900; Tsumugi's own comes in regular and bold,
    /// and draws other weights in the nearer of them.
    pub font_weight: u16,
    /// Which of the two themes colours the console and the window: `system`
    /// follows macOS as it switches between light and dark, `light` and `dark`
    /// keep to one. The Settings window chooses it, and the two themes, once
    /// for every environment and writes them here.
    pub appearance: Appearance,
    /// The theme drawn while the appearance is light, by name: one tsumugi comes
    /// with, or one in `themes/` beside `settings.toml`.
    pub light_theme: String,
    /// The theme drawn while the appearance is dark, named as the light one is.
    pub dark_theme: String,
    /// The one theme files written before themes followed the appearance
    /// name; read to keep their look, never written.
    #[serde(skip_serializing)]
    #[schemars(skip)]
    pub theme: Option<String>,
    /// Keep the Mac from idle sleep while a session or a shell runs, the window
    /// open or not; the display still sleeps.
    pub keep_awake: bool,
    /// Show a notification when a session starts waiting on you while the window
    /// is not in front; with several windows open, the one holding the lowest
    /// number tells.
    pub notify_waiting: bool,
    /// Show a notification when a session fails while the window is not in
    /// front, as `notify_waiting` does.
    pub notify_failed: bool,
    /// Show a notification when a session finishes its turn while the window
    /// is not in front, as `notify_waiting` does.
    pub notify_done: bool,
    /// Show a notification when a limit of an agent account passes 80%
    /// used while the window is not in front, as `notify_waiting` does.
    pub notify_usage: bool,
    /// Show how much of each usage limit is left; off, how much is used.
    pub usage_left: bool,
    /// Let a session's agent name its session once it has answered the first
    /// prompt, and again when the work turns to something else; a name given
    /// by hand, above the prompt or by renaming, is kept all the same.
    pub rename_sessions: bool,
    /// Whether the sessions and terminals a terminal host took with it, as it
    /// restarted, crashed or went with the machine, come back: offered by the
    /// window to bring back, at once, or never.
    pub restore_sessions: RestoreSessions,
    /// Close a command's tabs and splits when it finishes successfully; its
    /// output stays available from the tree. Off, its windows stay open too.
    pub close_successful_commands: bool,
    /// How a shell tab's shell starts.
    pub shell: ShellLaunch,
    /// How a Neovim tab's Neovim starts, as an agent does; an empty command
    /// runs `nvim`.
    pub neovim: Launch,
    /// Where scratch sessions run, `~` standing for your home folder; empty for
    /// Tsumugi's own folder.
    pub scratch_dir: String,
    /// Where the picker looks for directories to start a session in.
    pub picker: PickerSettings,
    /// The shell that floats over the window.
    pub floating_terminal: FloatingTerminalSettings,
    /// How the console takes keys and draws its cursor.
    pub console: ConsoleSettings,
    /// How much the details panel reads and shows.
    pub details: DetailsSettings,
    /// Machine resources sampled for a window that does not say which it
    /// shows; the Settings window chooses them once for every environment.
    pub usage: UsageSettings,
    /// What the details panel is made of, chosen once for every environment;
    /// see [details](details.md#components).
    pub panel: crate::details::DetailsLayout,
    /// Whether the setup script a repository lists runs in its new worktrees.
    pub worktree_setup: WorktreeSetupSettings,
    /// The agents asked for commit messages, pull requests, hook failures and
    /// conflicts from the Changes list, and how.
    pub ai_actions: AiActionsSettings,
    /// What sessions may use of the work machine: how many run at once, how
    /// their builds share, how large worktrees may grow.
    pub governor: GovernorSettings,
    /// The command that gates each repository, run once at a time on the work
    /// host for the sessions that ask.
    pub gate: GateSettings,
    /// The language a leader speaks to the user in, as a tag such as `ja` or
    /// `en`; empty for the language of the environment's locale. What the user
    /// does not read — prompts to sessions, commit messages, code — stays
    /// English whatever this says.
    pub language: String,
    /// Leader mode: the session that supervises the others, how many workers it
    /// keeps at work and how far its discretion goes.
    pub leader: LeaderSettings,
    /// The tiers a leader starts its workers by, each one a profile and a model.
    pub worker_tiers: WorkerTiers,
}

impl Serialize for Settings {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Settings::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for Settings {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let file = earlier::File::deserialize(deserializer)?;
        Settings::from_file(file).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct DelegationSettings {
    /// Whether sessions may open sessions beside them. Off, a start is refused
    /// without ending those already running; it is off while the file cannot be
    /// read.
    pub enabled: bool,
}

impl Default for DelegationSettings {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// Whether the setup script a repository's `tsumugi.toml` lists runs in its
/// new worktrees. A repository cannot say so itself: a file in it never runs
/// anything by being there.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct WorktreeSetupSettings {
    /// What a repository with no `[[worktree_setup.repositories]]` table of
    /// its own gets: `run`, `ask` to show the script and run it when you say
    /// so, or `skip`.
    pub run_policy: tsumugi_model::SetupRunPolicy,
    /// The repositories that differ, each `[[worktree_setup.repositories]]`
    /// table one of them.
    pub repositories: Vec<RepositorySetup>,
}

impl WorktreeSetupSettings {
    /// The policy of the repository whose primary checkout is `primary`.
    pub fn policy_for(&self, primary: &Path) -> tsumugi_model::SetupRunPolicy {
        self.repositories
            .iter()
            .find(|repository| Path::new(&repository.path) == primary)
            .map_or(self.run_policy, |repository| repository.run_policy)
    }
}

/// How one repository's new worktrees are set up.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct RepositorySetup {
    /// The repository's primary checkout, the directory holding its `.git`.
    pub path: String,
    /// What its new worktrees' setup script does.
    pub run_policy: tsumugi_model::SetupRunPolicy,
}

/// The fonts the console falls back to: tsumugi's own first, for the Nerd Font
/// glyphs a chosen font may lack, and a Japanese one for what a monospace
/// font leaves out.
pub const MONO: &str = r#""JetBrainsMono Nerd Font Mono", "SF Mono", Menlo, ui-monospace, "Hiragino Kaku Gothic ProN", monospace"#;

impl Default for Settings {
    fn default() -> Self {
        Self {
            profile: ProfileId::builtin(Agent::Claude),
            profiles: Agent::ALL.map(AgentProfile::builtin).to_vec(),
            unshared: Vec::new(),
            delegation: DelegationSettings::default(),
            zoom: 1.0,
            auto_scale: true,
            prompt_min_rows: 3,
            prompt_max_rows: 12,
            font_family: String::new(),
            font_size: 15.0,
            font_weight: 400,
            appearance: Appearance::System,
            light_theme: LIGHT_THEME.into(),
            dark_theme: DARK_THEME.into(),
            theme: None,
            keep_awake: true,
            notify_waiting: true,
            notify_failed: true,
            notify_done: true,
            notify_usage: true,
            usage_left: true,
            rename_sessions: true,
            restore_sessions: RestoreSessions::Ask,
            close_successful_commands: true,
            shell: ShellLaunch::default(),
            neovim: Launch::default(),
            scratch_dir: String::new(),
            picker: PickerSettings::default(),
            floating_terminal: FloatingTerminalSettings::default(),
            console: ConsoleSettings::default(),
            details: DetailsSettings::default(),
            usage: UsageSettings::default(),
            panel: crate::details::DetailsLayout::default(),
            worktree_setup: WorktreeSetupSettings::default(),
            ai_actions: AiActionsSettings::default(),
            governor: GovernorSettings::default(),
            gate: GateSettings::default(),
            language: String::new(),
            leader: LeaderSettings::default(),
            worker_tiers: WorkerTiers::default(),
        }
    }
}

/// The floating terminal: a shell over the window that every worktree shares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct FloatingTerminalSettings {
    /// The folder its shells start in, `~` standing for the home folder of the
    /// machine they run on; empty for that home folder.
    pub directory: String,
}

impl Default for FloatingTerminalSettings {
    fn default() -> Self {
        Self {
            directory: "~".into(),
        }
    }
}

/// Where the picker looks for directories to start a session in, beside where
/// sessions run and Claude Code has been used, and what else it searches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct PickerSettings {
    /// Offer the repositories ghq keeps, and its roots in the file tree.
    pub ghq: bool,
    /// Folders whose folders are offered, and which the file tree lists, `~`
    /// standing for your home folder.
    pub roots: Vec<String>,
    /// What the picker searches, each `[[picker.scopes]]` table one mix of
    /// sources: the first is where it opens, `Ctrl+g` steps through the
    /// others, and a scope with a `prefix` is entered by typing it first.
    /// Writing any replaces those the picker starts with
    /// ([picker](../tree.md#searching-more-than-sessions-and-directories)).
    pub scopes: Vec<PickerScope>,
}

impl Default for PickerSettings {
    fn default() -> Self {
        Self {
            ghq: true,
            roots: Vec::new(),
            scopes: PickerScope::defaults(),
        }
    }
}

/// Where the picker's rows come from: `sessions`, `directories`,
/// `worktrees`, `resources`, `files` or `actions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PickerSource {
    // The sessions, and the scratch session.
    Sessions,
    // The directories to start a session in, and repositories to clone.
    Directories,
    // Every worktree the tree lists.
    Worktrees,
    // What the projects keep as resources.
    Resources,
    // The files of the project shown, those git tracks or would track.
    Files,
    // Every action Tsumugi runs, as the keys and menus name them.
    Actions,
}

impl PickerSource {
    /// The group heading its rows are listed under.
    pub fn heading(self) -> &'static str {
        match self {
            PickerSource::Sessions => "Sessions",
            PickerSource::Directories => "Start in",
            PickerSource::Worktrees => "Worktrees",
            PickerSource::Resources => "Resources",
            PickerSource::Files => "Files",
            PickerSource::Actions => "Actions",
        }
    }
}

/// One mix of sources the picker searches at once.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct PickerScope {
    /// What the scope is called where the picker shows it.
    pub name: String,
    /// What is typed first to enter the scope instead of stepping to it;
    /// none when empty. What follows it is what is searched for.
    pub prefix: String,
    /// The sources it mixes, their rows listed in this order.
    pub sources: Vec<PickerSource>,
}

impl PickerScope {
    fn of(name: &str, prefix: &str, sources: &[PickerSource]) -> Self {
        Self {
            name: name.into(),
            prefix: prefix.into(),
            sources: sources.to_vec(),
        }
    }

    /// The scopes the picker starts with: going to a session or a directory,
    /// then each other source on its own, then the project's worktrees,
    /// resources and files together.
    pub fn defaults() -> Vec<Self> {
        use PickerSource::*;
        vec![
            Self::of("Go", "", &[Sessions, Directories]),
            Self::of("Actions", ">", &[Actions]),
            Self::of("Files", ";", &[Files]),
            Self::of("Resources", "*", &[Resources]),
            Self::of("Worktrees", ":", &[Worktrees]),
            Self::of("Project", "&", &[Worktrees, Resources, Files]),
        ]
    }

    /// The scope made sensible: its words trimmed, a source listed once.
    fn sane(mut self) -> Option<Self> {
        self.name = self.name.trim().to_string();
        self.prefix = self.prefix.trim().to_string();
        let mut seen = Vec::new();
        self.sources.retain(|s| {
            let new = !seen.contains(s);
            seen.push(*s);
            new
        });
        (!self.name.is_empty() && !self.sources.is_empty()).then_some(self)
    }
}

impl PickerSettings {
    /// The roots as paths, `~` standing for `home`.
    pub fn root_paths(&self, home: &std::path::Path) -> Vec<PathBuf> {
        self.roots
            .iter()
            .map(|r| r.trim())
            .filter(|r| !r.is_empty())
            .map(|r| expand(r, home))
            .collect()
    }
}

/// `path` with a leading `~` standing for `home`.
pub(crate) fn expand(path: &str, home: &std::path::Path) -> PathBuf {
    match path.strip_prefix('~') {
        Some(rest) => home.join(rest.trim_start_matches('/')),
        None => PathBuf::from(path),
    }
}

impl Settings {
    /// The settings made sensible: rows of at least one and the most no fewer
    /// than the fewest, a zoom among the levels' range.
    pub fn sane(mut self) -> Self {
        self.prompt_min_rows = self.prompt_min_rows.clamp(1, 40);
        self.prompt_max_rows = self.prompt_max_rows.clamp(self.prompt_min_rows, 60);
        let (least, most) = (
            crate::zoom::LEVELS[0],
            crate::zoom::LEVELS[crate::zoom::LEVELS.len() - 1],
        );
        self.zoom = self.zoom.clamp(least, most);
        self.font_family = match self.font_family.trim() {
            crate::fonts::FAMILY => String::new(),
            family => family.to_string(),
        };
        // Earlier versions offered a theme of tsumugi's own as "Tsumugi", in
        // Nightfox's colours on a darker ground.
        let named = |name: &str, otherwise: &str| match name.trim() {
            "" => otherwise.to_string(),
            "Tsumugi" => "Nightfox".to_string(),
            name => name.to_string(),
        };
        self.light_theme = named(&self.light_theme, LIGHT_THEME);
        self.dark_theme = named(&self.dark_theme, DARK_THEME);
        self.theme = self
            .theme
            .filter(|t| !t.trim().is_empty())
            .map(|t| named(&t, DARK_THEME));
        self.font_size = self.font_size.clamp(8.0, 32.0);
        self.font_weight = (self.font_weight.clamp(100, 900) + 50) / 100 * 100;
        let words = |c: Vec<String>| -> Vec<String> {
            c.into_iter()
                .map(|w| w.trim().to_string())
                .filter(|w| !w.is_empty())
                .collect()
        };
        self.shell.command = words(self.shell.command);
        for profile in &mut self.profiles {
            profile.launch.command = words(std::mem::take(&mut profile.launch.command));
        }
        self.neovim.command = words(self.neovim.command);
        self.language = self.language.trim().to_string();
        self.leader.max_workers = self.leader.max_workers.max(1);
        self.leader.model = self.leader.model.trim().to_string();
        for tier in [
            &mut self.worker_tiers.light,
            &mut self.worker_tiers.standard,
            &mut self.worker_tiers.heavy,
        ]
        .into_iter()
        .flatten()
        {
            tier.model = tier.model.trim().to_string();
        }
        let d = &mut self.details;
        d.log_commits = d.log_commits.clamp(1, 1000);
        d.diff_lines = d.diff_lines.clamp(1, 100_000);
        d.links_kept = d.links_kept.clamp(1, tsumugi_model::links::KEPT);
        d.links_shown = d.links_shown.max(1);
        self.picker.scopes = std::mem::take(&mut self.picker.scopes)
            .into_iter()
            .filter_map(PickerScope::sane)
            .collect();
        if self.picker.scopes.is_empty() {
            self.picker.scopes = PickerScope::defaults();
        }
        self
    }

    /// The theme in force while the system's appearance is `system`.
    pub fn theme_in(&self, system: ThemeKind) -> &str {
        let kind = match self.appearance {
            Appearance::System => system,
            Appearance::Light => ThemeKind::Light,
            Appearance::Dark => ThemeKind::Dark,
        };
        match kind {
            ThemeKind::Light => &self.light_theme,
            ThemeKind::Dark => &self.dark_theme,
        }
    }

    /// The settings with the one theme an earlier version named, of `kind`,
    /// taken as the theme of that kind and that appearance chosen, so the
    /// look stays as it was; settings without one as they are.
    pub fn migrated(mut self, kind: ThemeKind) -> Self {
        if let Some(theme) = self.theme.take() {
            match kind {
                ThemeKind::Light => {
                    self.light_theme = theme;
                    self.appearance = Appearance::Light;
                }
                ThemeKind::Dark => {
                    self.dark_theme = theme;
                    self.appearance = Appearance::Dark;
                }
            }
        }
        self
    }

    /// Where scratch sessions run, `~` standing for `home`, tsumugi's own
    /// folder being under `data`.
    pub fn scratch_folder(&self, home: &Path, data: &Path) -> PathBuf {
        match self.scratch_dir.trim() {
            "" => data.join("tsumugi").join("scratch"),
            dir => expand(dir, home),
        }
    }

    /// Where the floating terminal's shells start, `~` standing for `home`;
    /// `home` itself when no directory is chosen.
    pub fn floating_dir(&self, home: &Path) -> PathBuf {
        match self.floating_terminal.directory.trim() {
            "" => home.to_path_buf(),
            dir => expand(dir, home),
        }
    }

    /// The console's fonts as CSS names them: the chosen one first.
    pub fn console_font(&self) -> String {
        if self.font_family.is_empty() {
            return MONO.to_string();
        }
        let quoted = self.font_family.replace('\\', "\\\\").replace('"', "\\\"");
        format!("\"{quoted}\", {MONO}")
    }

    /// The settings `file` holds, as designed when there is none, or why
    /// they cannot be had, naming the file.
    pub fn read(file: &tsumugi_protocol::ConfigFile) -> Result<Self, String> {
        use tsumugi_protocol::FileText;
        let path = file.path.display();
        match &file.text {
            FileText::Missing => Ok(Settings::default()),
            FileText::Read { text } => {
                Settings::parse(text).map_err(|error| format!("Cannot load {path}: {error}"))
            }
            FileText::Unreadable { error } => Err(format!("Cannot read {path}: {error}")),
        }
    }

    /// The settings in `text`; what it does not name is as designed.
    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        let file = earlier::File::deserialize(text.parse::<toml::Table>()?)?;
        Ok(Settings::from_file(file)?.sane())
    }

    /// The settings `file` holds, its profiles checked.
    fn from_file(file: earlier::File) -> Result<Self, toml::de::Error> {
        let (profiles, rest) = file.split().map_err(serde::de::Error::custom)?;
        let settings = Settings::deserialize(rest)?;
        Ok(Settings {
            profile: profiles.profile,
            profiles: profiles.profiles,
            unshared: profiles.unshared,
            ..settings
        })
    }
}

/// The settings `text`, the file read again, holds when it reads and differs
/// from `current`; `None` otherwise, so a file half written or one tsumugi just
/// wrote itself changes nothing.
pub fn reread(current: &Settings, text: &str) -> Option<Settings> {
    Settings::parse(text).ok().filter(|s| s != current)
}

mod ai_actions;
mod earlier;
mod governor;
mod leader;
pub use ai_actions::{AiActionsSettings, Recipe, RepositoryRecipe, Resolved};
pub use governor::{BuildCache, GateSettings, GovernorSettings, RepositoryGate};
pub use leader::{
    Act, Discretion, DiscretionSettings, LeaderSettings, Tier, WorkerTier, WorkerTiers, language,
    language_instructions,
};

#[cfg(test)]
pub(crate) mod tests;
