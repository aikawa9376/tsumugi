use crate::execution::{TerminalStartRequest, resolve_terminal};
use tsumugi_model::{
    TerminalProgram,
    repository::{Rev, Spot},
};

#[test]
fn diff_editors_keep_the_users_configuration_and_receive_a_structured_scope() {
    let launch = resolve_terminal(
        &crate::settings::Settings::default(),
        TerminalStartRequest {
            floating: false,
            program: TerminalProgram::Neovim,
            cwd: "/repo".into(),
            diff: Some(Spot::Files(Rev::WorkingCopy)),
            ..Default::default()
        },
        "/bin/zsh",
    );
    assert_eq!(launch.spawn.program, "nvim");
    assert!(
        launch
            .spawn
            .args
            .iter()
            .any(|arg| arg.contains("DiffviewOpen"))
    );
    assert!(
        !launch
            .spawn
            .args
            .iter()
            .any(|arg| arg == "--clean" || arg == "-u")
    );
    assert!(
        launch
            .spawn
            .env
            .iter()
            .any(|(key, value)| key == "TSUMUGI_REVIEW" && value.contains("WorkingCopy"))
    );
}

#[test]
fn normally_configured_neovim_opens_plugin_or_builtin_diff_with_literal_paths() {
    let dir = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        assert!(
            std::process::Command::new("git")
                .args(args)
                .current_dir(dir.path())
                .env("GIT_AUTHOR_NAME", "Test")
                .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
                .env("GIT_COMMITTER_NAME", "Test")
                .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
                .output()
                .unwrap()
                .status
                .success()
        );
    };
    git(&["init", "-q"]);
    let file = "日本語 'quoted' | name.rs";
    std::fs::write(dir.path().join(file), "fn old() {}\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-qm", "initial"]);
    std::fs::write(dir.path().join(file), "fn new() {}\n").unwrap();
    std::fs::write(dir.path().join(".git/info/exclude"), "test.lua\n").unwrap();
    let script = dir.path().join("test.lua");
    std::fs::write(&script, r#"
local mode = vim.env.REVIEW_MODE
vim.g.user_configuration_loaded = true
if mode == 'plugin' then
  package.preload.diffview = function() return { open = function(args)
    assert(args[1] == 'HEAD' and args[2] == '--' and args[3] == vim.env.REVIEW_FILE)
    vim.g.review_plugin_opened = true
  end } end
elseif mode == 'lazy' then
  vim.api.nvim_create_user_command('DiffviewOpen', function() error('lazy command was opened twice') end, { nargs = '*' })
  package.preload['lazy.core.config'] = function() return { plugins = { ['diffview.nvim'] = {} } } end
  package.preload.lazy = function() return { load = function(opts)
    assert(opts.plugins[1] == 'diffview.nvim')
    package.preload.diffview = function() return { open = function() vim.g.review_plugin_opened = true end } end
  end } end
elseif mode == 'failed' then
  package.preload.diffview = function() return { open = function() error('broken plugin') end } end
end
vim.api.nvim_create_autocmd('VimEnter', { once = true, callback = function()
  vim.defer_fn(function()
    local ok, err = pcall(function()
      assert(vim.g.user_configuration_loaded)
      if mode == 'plugin' or mode == 'lazy' then
        assert(vim.g.review_plugin_opened)
      else
        local windows = vim.api.nvim_tabpage_list_wins(0)
        assert(#windows == 2, 'expected two diff windows')
        for _, win in ipairs(windows) do assert(vim.wo[win].diff) end
        local left = vim.api.nvim_win_get_buf(windows[1])
        local right = vim.api.nvim_win_get_buf(windows[2])
        assert(vim.api.nvim_buf_get_lines(left, 0, 1, false)[1] == (vim.env.REVIEW_OLD or ''), 'left: ' .. vim.inspect(vim.api.nvim_buf_get_lines(left, 0, -1, false)))
        assert(vim.api.nvim_buf_get_lines(right, 0, 1, false)[1] == vim.env.REVIEW_NEW)
        assert(vim.api.nvim_buf_line_count(left) == vim.api.nvim_buf_line_count(right))
      end
    end)
    if not ok then io.stderr:write(tostring(err)); vim.cmd('cquit 1') else vim.cmd('qa!') end
  end, 50)
end })
"#).unwrap();
    let scope = Spot::Diff(
        Rev::WorkingCopy,
        tsumugi_model::repository::Change {
            status: " M".into(),
            path: file.into(),
            from: None,
        },
    );
    for mode in ["absent", "plugin", "lazy", "failed", "all", "root"] {
        let scope = match mode {
            "all" => Spot::Files(Rev::WorkingCopy),
            "root" => Spot::Files(Rev::Commit("HEAD".into())),
            _ => scope.clone(),
        };
        let output = std::process::Command::new(tsumugi_nvim::program())
            .args(["--headless", "-i", "NONE", "--noplugin", "-u"])
            .arg(&script)
            .arg("-c")
            .arg(format!("lua {}", include_str!("../nvim/review.lua")))
            .current_dir(dir.path())
            .env("REVIEW_MODE", mode)
            .env("REVIEW_FILE", file)
            .env(
                "REVIEW_OLD",
                if mode == "root" { "" } else { "fn old() {}" },
            )
            .env(
                "REVIEW_NEW",
                if mode == "root" {
                    "fn old() {}"
                } else {
                    "fn new() {}"
                },
            )
            .env("TSUMUGI_REVIEW", serde_json::to_string(&scope).unwrap())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

/// What the plugin was asked to open for `scope`, run in `dir`.
fn plugin_args(dir: &std::path::Path, scope: &Spot) -> Vec<String> {
    let record = dir.join("args.json");
    let script = dir.join("record.lua");
    std::fs::write(
        &script,
        format!(
            r#"
package.preload.diffview = function() return {{ open = function(args)
  local file = io.open({record:?}, 'w')
  file:write(vim.json.encode(args))
  file:close()
end }} end
vim.api.nvim_create_autocmd('VimEnter', {{ once = true, callback = function()
  vim.defer_fn(function() vim.cmd('qa!') end, 50)
end }})
"#
        ),
    )
    .unwrap();
    let output = std::process::Command::new(tsumugi_nvim::program())
        .args(["--headless", "-i", "NONE", "--noplugin", "-u"])
        .arg(&script)
        .arg("-c")
        .arg(format!("lua {}", include_str!("../nvim/review.lua")))
        .current_dir(dir)
        .env("TSUMUGI_REVIEW", serde_json::to_string(scope).unwrap())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_str(&std::fs::read_to_string(record).unwrap()).unwrap()
}

#[test]
fn a_file_in_conflict_and_a_branch_are_handed_to_the_plugin_as_what_they_are() {
    let dir = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        assert!(
            std::process::Command::new("git")
                .args(["-c", "user.name=a", "-c", "user.email=a@b"])
                .args(args)
                .current_dir(dir.path())
                .output()
                .unwrap()
                .status
                .success(),
            "git {args:?}"
        );
    };
    git(&["init", "-q", "-b", "main"]);
    std::fs::write(dir.path().join(".git/info/exclude"), "*.lua\n*.json\n").unwrap();
    std::fs::write(dir.path().join("f.txt"), "one\n").unwrap();
    git(&["add", "f.txt"]);
    git(&["commit", "-qm", "one"]);
    let since = String::from_utf8(
        std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(dir.path())
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();
    let change = |status: &str| tsumugi_model::repository::Change {
        status: status.into(),
        path: "f.txt".into(),
        from: None,
    };
    assert_eq!(
        plugin_args(dir.path(), &Spot::Diff(Rev::WorkingCopy, change("UU"))),
        ["--", "f.txt"]
    );
    assert_eq!(
        plugin_args(
            dir.path(),
            &Spot::Diff(Rev::Branch(since.clone()), change("M"))
        ),
        [format!("{since}..HEAD"), "--".into(), "f.txt".into()]
    );
    assert_eq!(
        plugin_args(dir.path(), &Spot::Files(Rev::Branch(since.clone()))),
        [format!("{since}..HEAD")]
    );
}

#[test]
fn a_diff_tab_s_file_its_base_and_a_range_are_handed_to_the_plugin_as_what_they_are() {
    let dir = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(["-c", "user.name=a", "-c", "user.email=a@b"])
            .args(args)
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}");
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    };
    git(&["init", "-q", "-b", "main"]);
    std::fs::write(dir.path().join(".git/info/exclude"), "*.lua\n*.json\n").unwrap();
    std::fs::write(dir.path().join("f.txt"), "one\n").unwrap();
    git(&["add", "f.txt"]);
    git(&["commit", "-qm", "one"]);
    let base = git(&["rev-parse", "HEAD"]);
    let change = tsumugi_model::repository::Change {
        status: " M".into(),
        path: "f.txt".into(),
        from: None,
    };
    assert_eq!(
        plugin_args(
            dir.path(),
            &Spot::Lines(Rev::Base(base.clone()), change.clone())
        ),
        [base.clone(), "--".into(), "f.txt".into()]
    );
    assert_eq!(
        plugin_args(dir.path(), &Spot::Whole(Rev::WorkingCopy, change)),
        ["HEAD", "--", "f.txt"]
    );
    assert_eq!(
        plugin_args(
            dir.path(),
            &Spot::Stream(Rev::Range(base.clone(), "HEAD".into()))
        ),
        [format!("{base}..HEAD")]
    );
}

/// Runs the review of `f.txt`, changed on line 20 of 30, in `mode` with the
/// cursor asked on line 20, and fails unless it stands there in the new
/// side's window.
fn put_on_line(mode: &str) {
    let dir = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        assert!(
            std::process::Command::new("git")
                .args(["-c", "user.name=a", "-c", "user.email=a@b"])
                .args(args)
                .current_dir(dir.path())
                .output()
                .unwrap()
                .status
                .success(),
            "git {args:?}"
        );
    };
    let text = |changed: &str| {
        (1..=30)
            .map(|n| {
                if n == 20 {
                    changed.to_string()
                } else {
                    format!("line {n}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    };
    git(&["init", "-q", "-b", "main"]);
    std::fs::write(dir.path().join(".git/info/exclude"), "*.lua\n").unwrap();
    std::fs::write(dir.path().join("f.txt"), text("old")).unwrap();
    git(&["add", "f.txt"]);
    git(&["commit", "-qm", "one"]);
    std::fs::write(dir.path().join("f.txt"), text("new")).unwrap();
    let script = dir.path().join("test.lua");
    std::fs::write(
        &script,
        r#"
if vim.env.REVIEW_MODE == 'plugin' then
  -- Diffview draws its windows, the old left of the new, and says so.
  package.preload.diffview = function() return { open = function()
    vim.cmd.enew()
    vim.api.nvim_buf_set_lines(0, 0, -1, false, vim.fn.readfile('f.txt'))
    vim.cmd.diffthis()
    vim.cmd('leftabove vnew')
    vim.cmd.diffthis()
    vim.api.nvim_exec_autocmds('User', { pattern = 'DiffviewDiffBufWinEnter' })
  end } end
end
vim.api.nvim_create_autocmd('VimEnter', { once = true, callback = function()
  vim.defer_fn(function()
    local ok, err = pcall(function()
      local win = vim.api.nvim_get_current_win()
      local right = 0
      for _, w in ipairs(vim.api.nvim_tabpage_list_wins(0)) do
        right = math.max(right, vim.api.nvim_win_get_position(w)[2])
      end
      assert(vim.api.nvim_win_get_position(win)[2] == right, 'the cursor is not on the new side')
      local row = vim.api.nvim_win_get_cursor(win)[1]
      assert(row == 20, 'the cursor is on line ' .. row)
    end)
    if not ok then io.stderr:write(tostring(err)); vim.cmd('cquit 1') else vim.cmd('qa!') end
  end, 200)
end })
"#,
    )
    .unwrap();
    let scope = Spot::Diff(
        Rev::WorkingCopy,
        tsumugi_model::repository::Change {
            status: " M".into(),
            path: "f.txt".into(),
            from: None,
        },
    );
    let output = std::process::Command::new(tsumugi_nvim::program())
        .args(["--headless", "-i", "NONE", "--noplugin", "-u"])
        .arg(&script)
        .arg("-c")
        .arg(format!("lua {}", include_str!("../nvim/review.lua")))
        .current_dir(dir.path())
        .env("REVIEW_MODE", mode)
        .env("TSUMUGI_REVIEW", serde_json::to_string(&scope).unwrap())
        .env("TSUMUGI_REVIEW_LINE", "20")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{mode}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn the_built_in_diff_puts_the_cursor_on_the_line_asked_of_the_new_side() {
    put_on_line("absent");
}

#[test]
fn diffview_puts_the_cursor_on_the_line_asked_of_the_new_side() {
    put_on_line("plugin");
}

#[test]
fn the_line_asked_is_handed_to_the_diff_editor() {
    let launch = resolve_terminal(
        &crate::settings::Settings::default(),
        TerminalStartRequest {
            floating: false,
            program: TerminalProgram::Neovim,
            cwd: "/repo".into(),
            diff: Some(Spot::Files(Rev::WorkingCopy)),
            line: Some(12),
            ..Default::default()
        },
        "/bin/zsh",
    );
    assert!(
        launch
            .spawn
            .env
            .iter()
            .any(|(key, value)| key == "TSUMUGI_REVIEW_LINE" && value == "12")
    );
}
