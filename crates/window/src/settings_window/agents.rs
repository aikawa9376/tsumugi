//! The groups of options the Agents pages give each agent's new sessions.

use dioxus::prelude::*;
use tsumugi_model::config::{
    CodexOptions, CursorOptions, GenericOptions, OpenCodeOptions, PiOptions, PromptDelivery,
    SessionOptions, format_list, format_rules, parse_list, parse_rules,
};

use super::Edits;
use crate::components::{
    Select, SettingGroup, SettingRow, TextArea, TextField, Toggle, choices_of,
};

/// The permission modes Claude Code takes, as it names them, with what the
/// Settings window calls each; empty leaves it to Claude Code.
const MODES: [(&str, &str); 5] = [
    ("", "Claude Code's own"),
    ("default", "Ask before acting"),
    ("acceptEdits", "Accept edits"),
    ("plan", "Plan only"),
    ("bypassPermissions", "Bypass permissions"),
];

/// The models offered to choose from; another can be typed.
const MODELS: [&str; 3] = ["opus", "sonnet", "haiku"];

#[component]
pub(super) fn CodexGroup(
    title: String,
    options: CodexOptions,
    set: Callback<CodexOptions>,
) -> Element {
    let edits = Edits::new(&options, set);
    rsx! {
        SettingGroup { title: "{title}",
            SettingRow { label: "Model", hint: "Empty to use Codex's own model.",
                TextField {
                    value: options.model,
                    aria_label: "Model",
                    placeholder: "Codex's own",
                    onchange: edits.of(|o, value: String| o.model = value.trim().into()),
                }
            }
            SettingRow { label: "Reasoning effort", hint: "Use a level supported by the selected model, or leave empty for Codex's own.",
                TextField {
                    list: "codex-efforts",
                    aria_label: "Reasoning effort",
                    value: options.reasoning_effort,
                    placeholder: "Codex's own",
                    onchange: edits.of(|o, value: String| o.reasoning_effort = value.trim().into()),
                }
                datalist { id: "codex-efforts",
                    for value in ["low", "medium", "high", "xhigh", "max", "ultra"] {
                        option { key: "{value}", value: value }
                    }
                }
            }
            SettingRow { label: "Approval policy", hint: "When Codex asks before running a command. Never ask keeps sandbox restrictions in place.",
                Select { label: "Approval policy", value: options.approval_policy,
                    choices: choices_of([("", "Codex's own"), ("on-request", "Ask when needed"), ("never", "Never ask")]),
                    onchange: edits.of(|o, value: String| o.approval_policy = value),
                }
            }
            SettingRow { label: "Sandbox", hint: "Where commands can write. Full access removes the sandbox.",
                Select { label: "Sandbox", value: options.sandbox_mode,
                    choices: choices_of([("", "Codex's own"), ("read-only", "Read only"), ("workspace-write", "Workspace write"), ("danger-full-access", "Full access")]),
                    onchange: edits.of(|o, value: String| o.sandbox_mode = value),
                }
            }
        }
    }
}

/// How Pi sessions run: their model, thinking level and the tools they are
/// given, which is all Pi has for permissions.
#[component]
pub(super) fn PiGroup(title: String, options: PiOptions, set: Callback<PiOptions>) -> Element {
    let edits = Edits::new(&options, set);
    rsx! {
        SettingGroup { title: "{title}",
            SettingRow { label: "Model", hint: "As provider/id, the way Pi's --model takes it; empty for Pi's own.",
                TextField {
                    value: options.model,
                    aria_label: "Model",
                    placeholder: "Pi's own",
                    onchange: edits.of(|o, value: String| o.model = value.trim().into()),
                }
            }
            SettingRow { label: "Thinking", hint: "A level the selected model supports, or empty for Pi's own.",
                TextField {
                    list: "pi-thinking",
                    aria_label: "Thinking",
                    value: options.thinking,
                    placeholder: "Pi's own",
                    onchange: edits.of(|o, value: String| o.thinking = value.trim().into()),
                }
                datalist { id: "pi-thinking",
                    for value in ["off", "minimal", "low", "medium", "high", "xhigh", "max"] {
                        option { key: "{value}", value: value }
                    }
                }
            }
            SettingRow { label: "Tools", hint: "The tools they may use, separated by commas; empty for Pi's own set. Pi asks before no tool call and has no sandbox: a session given bash runs any command.",
                TextField {
                    value: "{format_list(&options.tools)}",
                    aria_label: "Tools",
                    placeholder: "read, bash, edit, write",
                    onchange: edits.of(|o, value: String| o.tools = parse_list(&value)),
                }
            }
        }
    }
}

/// How Cursor CLI sessions run: their model, the mode they start in and
/// whether they run in Cursor's sandbox.
#[component]
pub(super) fn CursorGroup(
    title: String,
    options: CursorOptions,
    set: Callback<CursorOptions>,
) -> Element {
    let edits = Edits::new(&options, set);
    rsx! {
        SettingGroup { title: "{title}",
            SettingRow { label: "Model", hint: "As `agent models` lists it; empty for Cursor's own.",
                TextField {
                    value: options.model,
                    aria_label: "Model",
                    placeholder: "Cursor's own",
                    onchange: edits.of(|o, value: String| o.model = value.trim().into()),
                }
            }
            SettingRow { label: "Mode", hint: "`plan` plans without editing, `ask` answers without editing; empty for Cursor's agent mode.",
                TextField {
                    list: "cursor-modes",
                    aria_label: "Mode",
                    value: options.mode,
                    placeholder: "agent",
                    onchange: edits.of(|o, value: String| o.mode = value.trim().into()),
                }
                datalist { id: "cursor-modes",
                    for value in ["plan", "ask"] {
                        option { key: "{value}", value: value }
                    }
                }
            }
            SettingRow { label: "Sandbox", hint: "`enabled` or `disabled`; empty for the sandbox setting Cursor keeps.",
                TextField {
                    list: "cursor-sandbox",
                    aria_label: "Sandbox",
                    value: options.sandbox,
                    placeholder: "Cursor's own",
                    onchange: edits.of(|o, value: String| o.sandbox = value.trim().into()),
                }
                datalist { id: "cursor-sandbox",
                    for value in ["enabled", "disabled"] {
                        option { key: "{value}", value: value }
                    }
                }
            }
        }
    }
}

/// How a command agent's first prompt reaches its program; the program
/// itself is the launch command below.
#[component]
pub(super) fn GenericGroup(
    title: String,
    options: GenericOptions,
    set: Callback<GenericOptions>,
) -> Element {
    let edits = Edits::new(&options, set);
    let delivery = match options.prompt {
        PromptDelivery::Argv => "argv",
        PromptDelivery::Separated => "separated",
        PromptDelivery::Flag => "flag",
        PromptDelivery::Typed => "typed",
    };
    rsx! {
        SettingGroup { title: "{title}",
            SettingRow { label: "First prompt", hint: "How the first prompt reaches the program: as its last argument, after `--`, after a flag, or not at all.",
                Select { label: "First prompt", value: delivery,
                    choices: choices_of([("argv", "Last argument"), ("separated", "After --"), ("flag", "After a flag"), ("typed", "Not sent")]),
                    onchange: edits.of(|o, value: String| {
                        o.prompt = match value.as_str() {
                            "separated" => PromptDelivery::Separated,
                            "flag" => PromptDelivery::Flag,
                            "typed" => PromptDelivery::Typed,
                            _ => PromptDelivery::Argv,
                        }
                    }),
                }
            }
            if options.prompt == PromptDelivery::Flag {
                SettingRow { label: "Prompt flag", hint: "The flag that takes the prompt.",
                    TextField {
                        value: options.prompt_flag,
                        aria_label: "Prompt flag",
                        placeholder: "-i",
                        onchange: edits.of(|o, value: String| o.prompt_flag = value.trim().into()),
                    }
                }
            }
        }
    }
}

/// How OpenCode sessions run: their model, the agent they start as, their
/// permission rules and whether they approve on their own, which is all
/// OpenCode has for permissions.
#[component]
pub(super) fn OpenCodeGroup(
    title: String,
    options: OpenCodeOptions,
    set: Callback<OpenCodeOptions>,
) -> Element {
    let edits = Edits::new(&options, set);
    rsx! {
        SettingGroup { title: "{title}",
            SettingRow { label: "Model", hint: "As provider/model, the way OpenCode lists its models; empty for OpenCode's own.",
                TextField {
                    value: options.model,
                    aria_label: "Model",
                    placeholder: "OpenCode's own",
                    onchange: edits.of(|o, value: String| o.model = value.trim().into()),
                }
            }
            SettingRow { label: "Agent", hint: "The primary agent they start as, one of OpenCode's or its login's own; empty for OpenCode's default.",
                TextField {
                    list: "opencode-agents",
                    aria_label: "Agent",
                    value: options.agent,
                    placeholder: "build",
                    onchange: edits.of(|o, value: String| o.agent = value.trim().into()),
                }
                datalist { id: "opencode-agents",
                    for value in ["build", "plan"] {
                        option { key: "{value}", value: value }
                    }
                }
            }
            SettingRow { label: "Permission rules", hint: "One a line: allow, ask or deny, then what a tool does, such as edit, shell or webfetch, then what it is done to, * for anything. They come after its login's own, and the last that matches a tool call decides. OpenCode has no sandbox: a session allowed shell runs any command.",
                TextArea {
                    value: "{format_rules(&options.permissions)}",
                    placeholder: "ask shell *",
                    onchange: edits.of(|o, value: String| o.permissions = parse_rules(&value)),
                }
            }
            SettingRow { label: "Approve on their own", hint: "Every tool call no rule denies runs without asking, as OpenCode's --auto does.",
                Toggle {
                    checked: options.auto,
                    onchange: edits.of(|o, on: bool| o.auto = on),
                }
            }
        }
    }
}

/// How sessions of one kind run: their model, permission mode, the tools
/// denied them, and whether Tsumugi's channel warning is confirmed for them.
#[component]
pub(super) fn SessionGroup(
    title: String,
    options: SessionOptions,
    set: Callback<SessionOptions>,
) -> Element {
    let edits = Edits::new(&options, set);
    let list = format!("models-{}", title.replace(' ', "-"));
    rsx! {
        SettingGroup { title: "{title}",
            SettingRow { label: "Model", hint: "The model they start with, as Claude Code's --model takes it; empty for Claude Code's own.",
                TextField {
                    list: "{list}",
                    value: "{options.model}",
                    placeholder: "Claude Code's own",
                    onchange: edits.of(|o, value: String| o.model = value.trim().to_string()),
                }
                datalist { id: "{list}",
                    for m in MODELS {
                        option { key: "{m}", value: "{m}" }
                    }
                }
            }
            SettingRow { label: "Permission mode", hint: "How they ask before they act.",
                Select { label: "Permission mode", value: options.permission_mode,
                    choices: choices_of(MODES),
                    onchange: edits.of(|o, value: String| o.permission_mode = value),
                }
            }
            SettingRow { label: "Tools denied", hint: "Tools they may not use, separated by commas.",
                TextField {
                    value: "{format_list(&options.disallowed_tools)}",
                    placeholder: "Edit, Write",
                    onchange: edits.of(|o, value: String| o.disallowed_tools = parse_list(&value)),
                }
            }
            SettingRow { label: "Confirm Tsumugi's channel warning", hint: "Confirm, once a launch, Claude Code's warning about loading development channels when it names Tsumugi's own channel alone; every other dialog stays yours.",
                Toggle {
                    checked: options.confirm_channel_warning,
                    onchange: edits.of(|o, on: bool| o.confirm_channel_warning = on),
                }
            }
        }
    }
}
