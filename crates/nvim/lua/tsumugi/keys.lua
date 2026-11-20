-- tsumugi's actions, named as a key binding names them, run from the prompt
-- with `:Tsumugi prompt.send`, and the keys tsumugi puts them on, which a
-- configuration maps, or others, as it likes.
local M = {}

--- Ask the window for `action`, as `:Tsumugi` does.
function M.run(action)
  vim.rpcnotify(vim.g.tsumugi_channel, "tsumugi_action", action)
end

--- What a mapping types to run `action`.
function M.ask(action)
  return "<Cmd>Tsumugi " .. action .. "<CR>"
end

-- Completion navigation must keep Neovim's keys rather than the editing
-- motions or application actions they otherwise run.
local function unless_completing(key, otherwise)
  return function()
    return vim.fn.pumvisible() == 1 and key or otherwise
  end
end

--- Leave `key` to completion, or run `action`.
function M.unless_completing(key, action)
  return unless_completing(key, M.ask(action))
end

local function editing()
  for key, motion in pairs({
    a = "<Home>", e = "<End>", b = "<Left>", f = "<Right>",
    n = "<Down>", p = "<Up>", h = "<BS>", d = "<Del>",
    k = "<C-o>D", u = "<C-u>", w = "<C-w>",
  }) do
    local lhs = "<C-" .. key .. ">"
    local rhs = "<C-g>U" .. motion
    if key == "n" or key == "p" then
      vim.keymap.set("i", lhs, unless_completing(lhs, rhs), { expr = true })
    else
      vim.keymap.set("i", lhs, rhs)
    end
  end
  vim.keymap.set("i", "<M-b>", "<C-g>U<S-Left>")
  vim.keymap.set("i", "<M-f>", "<C-g>U<S-Right>")
  for key, motion in pairs({ b = "<Left>", f = "<Right>", a = "<Home>", e = "<End>", d = "<Del>" }) do
    vim.keymap.set("c", "<C-" .. key .. ">", motion)
  end
end

-- Each action's key, name, whether completion keeps it, description, and
-- modes when it is not used in both insert and normal mode.
M.KEYS = {
  { "<C-CR>", "prompt.send", false, "Send the prompt" },
  { "<Tab>", "prompt.take_suggestion", true, "Take the session's suggested prompt into an empty prompt" },
  { "<S-Tab>", "console.send Shift+Tab", true, "Pass Shift+Tab to the session" },
  { "<C-c>", "console.send Escape", false, "Pass Escape to the session" },
  { "<C-p>", "prompt.history_prev", true, "Put the prompt sent before in the prompt", { "n" } },
  { "<C-n>", "prompt.history_next", true, "Put the prompt sent after in the prompt", { "n" } },
}

--- Make `:Tsumugi` and put tsumugi's actions on their keys.
function M.setup()
  editing()
  vim.api.nvim_create_user_command("Tsumugi", function(cmd)
    M.run(cmd.args)
  end, { nargs = "+", desc = "Run tsumugi's action" })
  for _, k in ipairs(M.KEYS) do
    local lhs, action, completing, desc = k[1], k[2], k[3], k[4]
    local modes = k[5] or { "i", "n" }
    if completing then
      vim.keymap.set(modes, lhs, M.unless_completing(lhs, action), { expr = true, desc = desc })
    else
      vim.keymap.set(modes, lhs, M.ask(action), { desc = desc })
    end
  end
end

return M
