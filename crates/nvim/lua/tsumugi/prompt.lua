-- Turns the user's Neovim into a prompt editor: the frame around the text is
-- hidden, the buffer is a scratch buffer of its own filetype, what is offered
-- as it is written is served as a language server, and every text change
-- reports how many screen lines the text needs. Runs once startup has
-- finished, after the user's configuration, and holds what it sets against
-- any later change.
local hold = require("tsumugi.hold")
local offer = require("tsumugi.offer")
local server = require("tsumugi.server")

local M = {}
local path_id, path_query = 0, nil

local function buffer()
  local b = vim.g.tsumugi_buf
  return b and vim.api.nvim_buf_is_valid(b) and b or vim.api.nvim_get_current_buf()
end

function M.freeze(frozen)
  vim.g.tsumugi_prompt_frozen = frozen
  vim.bo[buffer()].modifiable = not frozen
end

--- Window-owned edits remain possible while user input is frozen.
function M.edit(action, ...)
  local frozen = vim.g.tsumugi_prompt_frozen == true
  M.freeze(false)
  local ok, result = pcall(action, ...)
  M.freeze(frozen)
  if not ok then
    error(result)
  end
  return result
end

--- Empty the buffer `b`. Neovim's completion menu, if it shows, is offered
--- nothing so that it closes now: finishing it would put back what it was
--- completing once the text is gone.
function M.empty(b)
  vim.api.nvim_buf_set_lines(b, 0, -1, false, {})
  if vim.fn.pumvisible() == 1 then
    vim.fn.complete(1, {})
  end
end

function M.consume(text)
  local b = buffer()
  if table.concat(vim.api.nvim_buf_get_lines(b, 0, -1, false), "\n") ~= text then
    return false
  end
  return M.edit(function()
    M.empty(b)
    if vim.api.nvim_get_current_buf() == b then
      vim.api.nvim_win_set_cursor(0, { 1, 0 })
    end
    return true
  end)
end

local function available(before, entries)
  return offer.at(before, {
    candidates = vim.g.tsumugi_candidates,
    commands = vim.g.tsumugi_commands,
    root = vim.g.tsumugi_root,
    path_entries = entries,
  })
end

local function complete_paths(query, entries)
  local waiters = query.waiters
  query.waiters = {}
  for _, waiter in ipairs(waiters) do
    waiter.done(entries and available(waiter.before, entries) or nil)
  end
end

function M.root(root)
  if (vim.g.tsumugi_root or "") ~= root and path_query then
    complete_paths(path_query, nil)
    path_query = nil
  end
  vim.g.tsumugi_root = root
end

--- An answer for the exact query; late answers cannot populate another root.
function M.paths(id, entries)
  local query = path_query
  if not query or query.id ~= id or query.ready then
    return
  end
  if query.root ~= (vim.g.tsumugi_root or "") then
    complete_paths(query, nil)
    path_query = nil
    return
  end
  query.entries, query.ready, query.updated = entries, true, vim.uv.now()
  complete_paths(query, entries)
end

--- What the prompt offers for `before`, from what the window set, asking the
--- window on `chan` for the commands anew when a bare sigil is typed.
function M.offer(chan, before, done)
  local query = path_query
  local offered = available(before)
  if offered and offered.wants_commands then
    vim.rpcnotify(chan, "tsumugi_commands_wanted")
  end
  if offered and offered.paths then
    local root, typed = vim.g.tsumugi_root or "", offered.paths
    if not query or query.root ~= root or query.typed ~= typed or (query.ready and vim.uv.now() - query.updated > 1000) then
      if query then
        complete_paths(query, nil)
      end
      path_id = path_id + 1
      query = { id = path_id, root = root, typed = typed, waiters = {} }
      path_query = query
      vim.rpcnotify(chan, "tsumugi_paths_wanted", query.id, root, typed)
      vim.defer_fn(function()
        M.paths(query.id, {})
      end, 2000)
    end
    if query.ready then
      return available(before, query.entries)
    end
    if done then
      if #query.waiters >= 16 then
        return offered
      end
      query.waiters[#query.waiters + 1] = { before = before, done = done }
      return nil, true
    end
  end
  return offered
end

local function apply(chan)
  vim.cmd("silent! enew")
  local b = vim.api.nvim_get_current_buf()
  vim.g.tsumugi_buf = b
  local w = vim.api.nvim_get_current_win()
  hold.hold(function()
    return w
  end, b, {
    o = hold.everywhere(),
    wo = { eventignorewin = "" },
    bo = {
      buftype = "nofile",
      bufhidden = "hide",
      swapfile = false,
      filetype = "tsumugi-prompt",
      modifiable = function()
        return vim.g.tsumugi_prompt_frozen ~= true
      end,
      readonly = false,
    },
  })
  require("tsumugi.config").watch()
  -- Told as Neovim quits, however it is quit, and waited for: what is
  -- written goes to the Neovim the window starts next.
  vim.api.nvim_create_autocmd("ExitPre", {
    callback = function()
      if vim.api.nvim_buf_is_valid(b) then
        local text = table.concat(vim.api.nvim_buf_get_lines(b, 0, -1, false), "\n")
        pcall(vim.rpcrequest, chan, "tsumugi_leaving", text, vim.fn.winsaveview())
      end
    end,
  })
  server.start(b, function(before, done)
    return M.offer(chan, before, done)
  end)
  -- Told on every change, from wherever it comes: typed, pasted, or set by
  -- the window.
  local shell = false
  vim.api.nvim_buf_attach(b, false, {
    on_lines = function()
      vim.schedule(function()
        if not vim.api.nvim_buf_is_valid(b) then
          return
        end
        local now = (vim.api.nvim_buf_get_lines(b, 0, 1, false)[1] or ""):sub(1, 1) == "!"
        if now ~= shell then
          shell = now
          vim.rpcnotify(chan, "tsumugi_shell", now)
        end
      end)
    end,
  })
  local function report()
    local ok, h = pcall(vim.api.nvim_win_text_height, 0, {})
    local need = ok and h.all or vim.api.nvim_buf_line_count(0)
    -- A window grown to fit the text keeps the scroll it needed while it was
    -- smaller, hiding the first lines above an empty one.
    if need <= vim.api.nvim_win_get_height(0) and vim.fn.line("w0") > 1 then
      vim.fn.winrestview({ topline = 1 })
    end
    vim.rpcnotify(chan, "tsumugi_height", need)
  end
  local function view()
    vim.rpcnotify(chan, "tsumugi_view", vim.fn.line("w0") - 1, vim.api.nvim_buf_line_count(0))
  end
  vim.api.nvim_create_autocmd(
    { "TextChanged", "TextChangedI", "TextChangedP", "VimResized", "BufEnter", "WinResized" },
    { callback = report }
  )
  vim.api.nvim_create_autocmd(
    { "WinScrolled", "TextChanged", "TextChangedI", "BufEnter" },
    { callback = view }
  )
  report()
  vim.cmd("startinsert")
end

--- Set the prompt up for the window on `chan`, once startup has finished.
function M.setup(chan)
  vim.g.tsumugi_channel = chan
  if vim.v.vim_did_enter == 1 then
    apply(chan)
  else
    vim.api.nvim_create_autocmd("VimEnter", {
      once = true,
      callback = function()
        apply(chan)
      end,
    })
  end
end

return M
