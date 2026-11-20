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
