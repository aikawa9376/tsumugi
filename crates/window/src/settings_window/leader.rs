//! Leader mode: the language a leader speaks in, its own session and workers,
//! the tiers workers are started by and how far its discretion goes.

use dioxus::prelude::*;
use tsumugi_core::settings::{Act, Discretion, Tier, WorkerTier};

use super::{Change, set_number};
use crate::components::{NumberField, SettingGroup, SettingRow, TextField, Toggle};
use crate::settings::Settings;

/// The tier as the settings hold it with `edit` made of it; a tier left with
/// neither a profile nor a model is not defined.
fn edited_tier(
    tier: Option<&WorkerTier>,
    edit: impl FnOnce(&mut WorkerTier),
) -> Option<WorkerTier> {
    let mut next = tier.cloned().unwrap_or_default();
    edit(&mut next);
    next.model = next.model.trim().to_string();
    (next.profile.is_some() || !next.model.is_empty()).then_some(next)
}

fn tier_mut(settings: &mut Settings, tier: Tier) -> &mut Option<WorkerTier> {
    match tier {
        Tier::Light => &mut settings.worker_tiers.light,
        Tier::Standard => &mut settings.worker_tiers.standard,
        Tier::Heavy => &mut settings.worker_tiers.heavy,
    }
}

fn held(settings: &Settings, tier: Tier) -> Option<&WorkerTier> {
    settings.worker_tiers.resolve(tier).ok()
}

#[component]
pub(super) fn LeaderPane(now: Settings, change: Callback<Change>) -> Element {
    rsx! {
        SettingGroup { title: "Language",
            SettingRow { label: "Language", hint: "What a leader says to you, such as ja or en; empty for the language of the environment's locale. What you do not read stays English.",
                TextField {
                    value: "{now.language}",
                    placeholder: "environment's locale",
                    onchange: move |value: String| {
                        let language = value.trim().to_string();
                        change(Box::new(move |s: &mut Settings| s.language = language));
                    },
                }
            }
        }
        SettingGroup { title: "Leader",
            SettingRow { label: "Profile", hint: "The profile the leader's own session starts from, by id; empty for the profile new sessions start from.",
                TextField {
                    value: "{now.leader.profile.as_ref().map(|p| p.to_string()).unwrap_or_default()}",
                    placeholder: "default profile",
                    onchange: move |value: String| {
                        let profile = value.trim().parse().ok();
                        change(Box::new(move |s: &mut Settings| s.leader.profile = profile));
                    },
                }
            }
            SettingRow { label: "Model", hint: "The model the leader's own session starts; empty for its agent's default.",
                TextField {
                    value: "{now.leader.model}",
                    onchange: move |value: String| {
                        let model = value.trim().to_string();
                        change(Box::new(move |s: &mut Settings| s.leader.model = model));
                    },
                }
            }
            SettingRow { label: "Workers at once", hint: "The most workers a leader keeps at work.",
                NumberField {
                    min: "1", max: "32",
                    value: "{now.leader.max_workers}",
                    onchange: set_number::<u32>(change, |s, n| s.leader.max_workers = n),
                }
            }
        }
        SettingGroup { title: "Worker tiers",
            for tier in Tier::ALL {
                SettingRow {
                    key: "{tier.name()}",
                    label: tier.name().to_string(),
                    hint: match tier {
                        Tier::Light => "Small tasks.",
                        Tier::Standard => "Tasks of ordinary size.",
                        Tier::Heavy => "Large or hard tasks.",
                    }.to_string(),
                    TextField {
                        value: "{held(&now, tier).and_then(|t| t.profile.as_ref()).map(|p| p.to_string()).unwrap_or_default()}",
                        placeholder: "profile",
                        aria_label: format!("{} profile", tier.name()),
                        onchange: move |value: String| {
                            let profile = value.trim().parse().ok();
                            change(Box::new(move |s: &mut Settings| {
                                let next = edited_tier(tier_mut(s, tier).as_ref(), |t| t.profile = profile);
                                *tier_mut(s, tier) = next;
                            }));
                        },
                    }
                    TextField {
                        value: "{held(&now, tier).map(|t| t.model.clone()).unwrap_or_default()}",
                        placeholder: "model",
                        aria_label: format!("{} model", tier.name()),
                        onchange: move |value: String| {
                            change(Box::new(move |s: &mut Settings| {
                                let next = edited_tier(tier_mut(s, tier).as_ref(), |t| t.model = value);
                                *tier_mut(s, tier) = next;
                            }));
                        },
                    }
                }
            }
        }
        SettingGroup { title: "Discretion: what the leader decides itself",
            for act in Act::ALL {
                SettingRow {
                    key: "{act.label()}",
                    label: act.label().to_string(),
                    hint: if now.leader.discretion.of(act) == Discretion::Ask {
                        "It asks you first.".to_string()
                    } else {
                        "It decides.".to_string()
                    },
                    Toggle {
                        checked: now.leader.discretion.of(act) == Discretion::Decide,
                        onchange: move |on: bool| {
                            let how = if on { Discretion::Decide } else { Discretion::Ask };
                            change(Box::new(move |s: &mut Settings| s.leader.discretion.set(act, how)));
                        },
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tier_with_neither_a_profile_nor_a_model_is_not_defined() {
        assert_eq!(edited_tier(None, |t| t.model = "  ".into()), None);
        let kept = edited_tier(None, |t| t.model = " haiku ".into()).unwrap();
        assert_eq!(kept.model, "haiku");
        let emptied = edited_tier(Some(&kept), |t| t.model.clear());
        assert_eq!(emptied, None);
    }

    #[test]
    fn a_tier_is_written_into_its_own_place() {
        let mut settings = Settings::default();
        *tier_mut(&mut settings, Tier::Heavy) = edited_tier(None, |t| t.model = "opus".into());
        assert_eq!(settings.worker_tiers.heavy.as_ref().unwrap().model, "opus");
        assert!(settings.worker_tiers.light.is_none());
        assert_eq!(held(&settings, Tier::Heavy).unwrap().model, "opus");
        assert!(held(&settings, Tier::Light).is_none());
    }
}
