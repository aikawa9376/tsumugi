mod log;

fn main() {
    log::init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    #[cfg(all(target_os = "linux", feature = "desktop"))]
    if args.first().is_some_and(|arg| arg == "__kitty_display") {
        std::process::exit(tsumugi_window::kitty_display(&args[1..]));
    }
    if args.first().is_some_and(|arg| arg == "__startup_lease") {
        std::process::exit(tsumugi_host::startup_lease(&args[1..]));
    }
    // Ahead of settling the instance: the program it keeps is handed the
    // environment exactly as its terminal was.
    if args.first().is_some_and(|arg| arg == tsumugi_host::KEEP) {
        std::process::exit(tsumugi_host::keep(&args[1..]));
    }
    #[cfg(windows)]
    if args.first().is_some_and(|arg| arg == tsumugi_host::HOLD) {
        std::process::exit(tsumugi_host::hold(&args[1..]));
    }
    use tsumugi_host::cli::{
        Command, ROUTING, RemoteCommand, USAGE, command, named_instance, routing_and_credential,
        set_aside, settle_instance,
    };
    // Run from inside an agent's account, such as by its MCP server or a
    // command its session runs, tsumugi finds its instance where it is
    // outside the account, not where the account moved the agent's folders.
    for (name, value) in tsumugi_agents::own_login::outside(std::env::vars_os()) {
        // SAFETY: no other thread exists yet.
        unsafe {
            match value {
                Some(value) => std::env::set_var(&name, value),
                None => std::env::remove_var(&name),
            }
        }
    }
    let asked = command(&args);
    // Ahead of settling the instance: the agent's program is handed the
    // environment exactly as its login leaves it.
    if let Some(Command::InOwnLogin(rest)) = &asked {
        std::process::exit(tsumugi_agents::own_login::run(rest));
    }
    // An instance named on the command line wins over the one the session it
    // is typed in belongs to.
    if let Some(Command::Issue(rest) | Command::Schedule(rest)) = &asked
        && named_instance(rest).is_some()
    {
        for name in routing_and_credential() {
            // SAFETY: no other thread exists yet.
            unsafe { std::env::remove_var(name) };
        }
    }
    for name in set_aside(|name| std::env::var(name).ok()) {
        // SAFETY: no other thread exists yet.
        unsafe { std::env::remove_var(name) };
    }
    let given = match &asked {
        Some(Command::Window { instance, .. } | Command::Remote { instance, .. }) => {
            instance.clone()
        }
        Some(Command::Issue(rest) | Command::Schedule(rest)) => named_instance(rest),
        Some(Command::Host(_) | Command::Daemon(_)) => std::env::var("TSUMUGI_WORK_INSTANCE").ok(),
        _ => None,
    };
    let new = matches!(asked, Some(Command::Window { new: true, .. }));
    let work_route = std::env::var_os("TSUMUGI_WORK_INSTANCE").is_some();
    // Every process started from here belongs to the instance it settles on.
    if let Err(e) = settle_instance(given) {
        eprintln!("tsumugi: {e}");
        std::process::exit(2);
    }
    let window_asked = cfg!(feature = "desktop") && matches!(asked, Some(Command::Window { .. }));
    if window_asked {
        // The window routes each environment it shows itself, so it starts
        // its hosts with their routing named; one inherited from a session
        // it was opened from would name another environment's.
        if let Err(error) = tsumugi_connect::environments::settle(
            &tsumugi_host::paths::instance(),
            &tsumugi_host::paths::instance_state(),
        ) {
            eprintln!("tsumugi: {error}");
            std::process::exit(2);
        }
        for name in ROUTING {
            // SAFETY: environment routing is settled before any runtime creates threads.
            unsafe { std::env::remove_var(name) };
        }
    } else if !work_route
        && std::env::var_os("TSUMUGI_ENVIRONMENT").is_none()
        && asked.as_ref().is_some_and(Command::routed)
    {
        let binding =
            tsumugi_connect::environments::Binding::load(&tsumugi_host::paths::instance_state());
        let binding = match binding {
            Ok(binding) => binding,
            Err(error) => {
                eprintln!("tsumugi: {error}");
                std::process::exit(2);
            }
        };
        if let Some(binding) = binding {
            // SAFETY: environment routing is settled before any runtime creates threads.
            unsafe {
                if binding.target == tsumugi_connect::Target::Local {
                    let work = binding.work_instance.as_deref().unwrap_or("default");
                    if binding.scope.is_some() || work != tsumugi_host::paths::instance() {
                        let service = tsumugi_host::paths::instance_dir(
                            &tsumugi_host::paths::state_dir(),
                            work,
                        );
                        std::env::set_var("TSUMUGI_WORK_INSTANCE", work);
                        std::env::set_var("TSUMUGI_HOST", service.join("host/host.sock"));
                        std::env::set_var("TSUMUGI_DAEMON", service.join("felis/daemon.sock"));
                    }
                }
                match binding.scope {
                    Some(scope) => {
                        std::env::set_var("TSUMUGI_ENVIRONMENT", scope);
                    }
                    None => {
                        std::env::remove_var("TSUMUGI_ENVIRONMENT");
                    }
                }
            }
        }
    }
    if !tsumugi_host::remote::active()
        && std::env::var_os("TSUMUGI_ENVIRONMENT").is_none()
        && !work_route
        && matches!(
            &asked,
            Some(
                Command::Session(_)
                    | Command::Issue(_)
                    | Command::Schedule(_)
                    | Command::Gate(_)
                    | Command::Cleanup(_)
                    | Command::Terminal(_)
                    | Command::Ports(_)
                    | Command::Mcp(_)
                    | Command::Editor(_)
                    | Command::Daemon(_)
                    | Command::Host(_)
            )
        )
    {
        let path = tsumugi_host::paths::instance_state().join("execution.toml");
        let target =
            tsumugi_connect::environments::Binding::load(&tsumugi_host::paths::instance_state())
                .and_then(|binding| {
                    binding.map_or_else(
                        || tsumugi_connect::Target::load(&path),
                        |binding| Ok(binding.target),
                    )
                });
        let refused = match target {
            Ok(tsumugi_connect::Target::Local) => None,
            Ok(tsumugi_connect::Target::Native { .. }) => Some("Remote execution is selected; run this command on the work machine, or use tsumugi connect to check its connection".to_string()),
            Err(text) => Some(text),
        };
        if let Some(text) = refused {
            eprintln!("tsumugi: {text}");
            std::process::exit(2);
        }
    }
    if matches!(
        asked,
        Some(
            Command::Daemon(_)
                | Command::Host(_)
                | Command::Remote {
                    operation: RemoteCommand::Host | RemoteCommand::Daemon,
                    ..
                }
        )
    ) {
        // SAFETY: the host runtime has not started any threads.
        unsafe {
            std::env::remove_var("TSUMUGI_ENVIRONMENT");
        }
        let names = std::env::vars_os().map(|(key, _)| key.to_string_lossy().into_owned());
        for key in tsumugi_host::env::inherited(names) {
            // SAFETY: the host runtime has not created any threads yet.
            unsafe { std::env::remove_var(key) };
        }
    }
    match asked {
        None => {
            eprintln!(
                "{}",
                USAGE.replacen(
                    "usage: tsumugi",
                    concat!("usage: ", env!("CARGO_BIN_NAME")),
                    1
                )
            );
            std::process::exit(2);
        }
        Some(Command::Version) => {
            println!("{} {}", env!("CARGO_BIN_NAME"), env!("CARGO_PKG_VERSION"));
            return;
        }
        Some(Command::Connect) => {
            let path = tsumugi_host::paths::instance_state().join("execution.toml");
            let result = tsumugi_connect::diagnose(&path);
            match result {
                Ok(text) => println!("{text}"),
                Err(text) => {
                    eprintln!("tsumugi: {text}");
                    std::process::exit(1);
                }
            }
            return;
        }
        Some(Command::Discover) => {
            match tsumugi_connect::discover_cli() {
                Ok(lines) if lines.is_empty() => {
                    println!("No work machine announced itself on this network")
                }
                Ok(lines) => lines.iter().for_each(|line| println!("{line}")),
                Err(text) => {
                    eprintln!("tsumugi: {text}");
                    std::process::exit(1);
                }
            }
            return;
        }
        Some(Command::Pair { address, name }) => {
            match tsumugi_connect::pair_cli(&address, name.as_deref(), |text| println!("{text}")) {
                Ok(text) => println!("{text}"),
                Err(text) => {
                    eprintln!("tsumugi: {text}");
                    std::process::exit(1);
                }
            }
            return;
        }
        Some(Command::Session(rest)) => std::process::exit(tsumugi_host::session_cli(&rest)),
        Some(Command::Skills(rest)) => std::process::exit(tsumugi_host::skills::cli(&rest)),
        Some(Command::Issue(rest)) => std::process::exit(tsumugi_host::issue_cli(&rest)),
        Some(Command::Schedule(rest)) => std::process::exit(tsumugi_host::schedule_cli(&rest)),
        Some(Command::Gate(rest)) => std::process::exit(tsumugi_host::gate_cli("gate", &rest)),
        Some(Command::Cleanup(rest)) => {
            std::process::exit(tsumugi_host::gate_cli("cleanup", &rest))
        }
        Some(Command::Mcp(rest)) => {
            std::process::exit(tsumugi_host::delegation::bridge::cli(&rest))
        }
        Some(Command::Terminal(rest)) => std::process::exit(tsumugi_host::terminal_cli(&rest)),
        Some(Command::Ports(rest)) => std::process::exit(tsumugi_host::ports_cli(&rest)),
        Some(Command::Editor(rest)) => std::process::exit(editor(&rest)),
        Some(Command::Daemon(asked)) => std::process::exit(tsumugi_host::daemon_cli(asked)),
        Some(Command::Host(asked)) => std::process::exit(tsumugi_host::host_cli(asked)),
        Some(Command::Remote { operation, .. }) => {
            std::process::exit(tsumugi_host::remote::cli(operation))
        }
        Some(Command::Window { .. }) => {}
        Some(Command::InOwnLogin(_)) => unreachable!("run before the instance is settled"),
    }
    window(new);
}

#[cfg(feature = "desktop")]
fn editor(args: &[String]) -> i32 {
    tsumugi_window::editor::cli(args)
}

#[cfg(not(feature = "desktop"))]
fn editor(_: &[String]) -> i32 {
    eprintln!("tsumugi: this build has no window to take editor input");
    2
}

#[cfg(not(feature = "desktop"))]
fn window(_: bool) {
    eprintln!("tsumugi: this build has no window; use `tsumugi host` to serve the work");
    std::process::exit(2);
}

#[cfg(feature = "desktop")]
fn window(new: bool) {
    match tsumugi_window::open_window(new) {
        // An open window came to the front.
        Ok(false) => return,
        Ok(true) => {}
        Err(e) => {
            eprintln!("tsumugi: cannot open a window: {e}");
            std::process::exit(1);
        }
    }
    // Opened from the Finder, there is no terminal and no PATH of the user's
    // own; the login shell knows where their tools are.
    if std::env::var_os("TERM").is_none()
        && let Some(path) = tsumugi_host::env::login_path(
            &tsumugi_host::env::login_shell().unwrap_or_else(|| "/bin/zsh".into()),
        )
    {
        // SAFETY: no other thread exists yet.
        unsafe { std::env::set_var("PATH", path) };
    }
    let names = std::env::vars_os().map(|(k, _)| k.to_string_lossy().into_owned());
    for k in tsumugi_host::env::inherited(names) {
        // SAFETY: no other thread exists yet.
        unsafe { std::env::remove_var(k) };
    }
    tsumugi_window::launch(tsumugi_window::Build {
        build: tsumugi_host::standing::build(),
        described: tsumugi_host::standing::DESCRIBED,
    });
}
