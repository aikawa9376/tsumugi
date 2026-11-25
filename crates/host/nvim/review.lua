local scope = vim.json.decode(vim.env.TSUMUGI_REVIEW)
vim.env.TSUMUGI_REVIEW = nil
-- The line of the file, on its new side, the cursor is put on.
local line = tonumber(vim.env.TSUMUGI_REVIEW_LINE or '')
vim.env.TSUMUGI_REVIEW_LINE = nil

-- Put the cursor of `win` on the line asked, once its buffer holds it;
-- a buffer read in the background is waited for a while.
local function go_to(win, tries)
  if not line or not vim.api.nvim_win_is_valid(win) then return end
  local count = vim.api.nvim_buf_line_count(vim.api.nvim_win_get_buf(win))
  if count < line and (tries or 0) < 20 then
    vim.defer_fn(function() go_to(win, (tries or 0) + 1) end, 50)
    return
  end
  vim.api.nvim_set_current_win(win)
  pcall(vim.api.nvim_win_set_cursor, win, { math.min(line, count), 0 })
  vim.cmd('normal! zz')
end

-- The window of the new side of the diff Diffview draws in the tab: the
-- second from the left, the right of two and the middle of three.
local function new_side()
  local windows = vim.tbl_filter(function(w) return vim.wo[w].diff end, vim.api.nvim_tabpage_list_wins(0))
  table.sort(windows, function(a, b) return vim.api.nvim_win_get_position(a)[2] < vim.api.nvim_win_get_position(b)[2] end)
  return windows[math.min(2, #windows)]
end

local function open()
  local pair = scope.Diff or scope.Lines or scope.Whole
  local rev = scope.Files or scope.Stream or (pair and pair[1]) or 'WorkingCopy'
  local change = pair and pair[2]
  local range = type(rev) == 'table' and rev.Range or nil
  -- The worktree against this commit, committed or not.
  local against = type(rev) == 'table' and rev.Base or nil
  local since = type(rev) == 'table' and (rev.Branch or (range and range[1])) or nil
  local commit = type(rev) == 'table' and (rev.Commit or (rev.Branch and 'HEAD') or (range and range[2])) or nil
  local unmerged = change and ({ DD = true, AU = true, UD = true, UA = true, DU = true, AA = true, UU = true })[change.status]
  local function git(args)
    local command = { 'git', '--literal-pathspecs' }
    vim.list_extend(command, args)
    if vim.system then
      local result = vim.system(command, { text = false }):wait()
      return result.code == 0 and result.stdout or nil
    end
    local text = vim.fn.system(command)
    return vim.v.shell_error == 0 and text:gsub('\1', '\0') or nil
  end
  local function paths(text)
    return vim.split(text or '', '\0', { plain = true, trimempty = true })
  end
  local configured, config = pcall(require, 'lazy.core.config')
  if configured and config.plugins and config.plugins['diffview.nvim'] then
    pcall(function() require('lazy').load({ plugins = { 'diffview.nvim' } }) end)
  end
  -- Requiring the module also triggers module-based lazy loading. Pass paths
  -- as data to the plugin rather than through Ex command parsing.
  local available, diffview = pcall(require, 'diffview')
  if available and type(diffview) == 'table' and type(diffview.open) == 'function' then
    local compared = since and (since .. '..' .. commit) or (commit and (commit .. '^!') or against or 'HEAD')
    if git({ 'rev-parse', '--verify', since or (commit and (commit .. '^')) or against or 'HEAD' }) then
      local args = { compared }
      if unmerged then
        -- A file in conflict is Diffview's to merge: it lists it apart from
        -- the changes, which a range would leave out.
        args = { '--', change.path }
      elseif change then
        vim.list_extend(args, { '--', change.path })
        if change.from and change.from ~= vim.NIL then table.insert(args, change.from) end
      end
      if line and change then
        -- Diffview opens the file in the background and then moves the
        -- cursor as it is configured to, so the cursor is put on the line
        -- once its windows have settled, for each that opens while it
        -- starts.
        local clock = vim.uv or vim.loop
        local until_ms = clock.now() + 2000
        local group = vim.api.nvim_create_augroup('TsumugiReviewLine', { clear = true })
        vim.api.nvim_create_autocmd('User', { group = group, pattern = 'DiffviewDiffBufWinEnter', callback = function()
          if clock.now() > until_ms then return true end
          vim.defer_fn(function()
            local win = new_side()
            if win then go_to(win) end
          end, 100)
        end })
      end
      local ok, err = pcall(diffview.open, args)
      if ok then return end
      vim.notify('DiffviewOpen failed: ' .. tostring(err) .. '. Opening built-in diff.', vim.log.levels.WARN)
    end
  end
  local base = since or (commit and (commit .. '^')) or against or 'HEAD'
  local files
  if change then
    files = { change.path }
  elseif since then
    files = paths(git({ 'diff', '--no-ext-diff', '--no-textconv', '--no-renames', '--name-only', '-z', since, commit, '--' }))
  elseif commit then
    files = paths(git({ 'diff-tree', '--root', '--no-commit-id', '--no-renames', '-r', '--name-only', '-z', commit, '--' }))
  else
    files = paths(git({ 'diff', '--no-ext-diff', '--no-textconv', '--no-renames', '--name-only', '-z', base, '--' }))
    if not git({ 'rev-parse', '--verify', base }) then
      files = paths(git({ 'ls-files', '-z' }))
    end
    vim.list_extend(files, paths(git({ 'ls-files', '--others', '--exclude-standard', '-z' })))
  end
  local seen = {}
  local function scratch(name, text, file)
    vim.cmd.enew()
    vim.bo.buftype = 'nofile'
    vim.bo.bufhidden = 'wipe'
    vim.bo.swapfile = false
    vim.api.nvim_buf_set_name(0, name)
    local lines = vim.split(text or '', '\n', { plain = true })
    if lines[#lines] == '' then table.remove(lines) end
    vim.api.nvim_buf_set_lines(0, 0, -1, false, lines)
    vim.bo.filetype = vim.filetype.match({ filename = file }) or ''
    vim.bo.modified = false
    vim.bo.modifiable = false
    vim.cmd.diffthis()
  end
  local first = true
  local shown
  for _, file in ipairs(files) do
    if not seen[file] then
      seen[file] = true
      if not first then vim.cmd.tabnew() end
      first = false
      local old = change and change.from ~= vim.NIL and change.from or file
      scratch('tsumugi-diff://' .. vim.fn.tabpagenr() .. '/base/' .. old, git({ 'show', base .. ':' .. old }), file)
      vim.cmd('rightbelow vnew')
      if commit then
        scratch('tsumugi-diff://' .. vim.fn.tabpagenr() .. '/commit/' .. file, git({ 'show', commit .. ':' .. file }), file)
      elseif vim.fn.filereadable(file) == 1 then
        vim.cmd('edit ' .. vim.fn.fnameescape(vim.fn.fnamemodify(file, ':p')))
        vim.cmd.diffthis()
      else
        scratch('tsumugi-diff://' .. vim.fn.tabpagenr() .. '/deleted/' .. file, '', file)
      end
      shown = shown or vim.api.nvim_get_current_win()
    end
  end
  if first then vim.notify('No changes', vim.log.levels.INFO) end
  if shown and change then
    vim.cmd.tabfirst()
    go_to(shown)
  end
end

if vim.v.vim_did_enter == 1 then
  open()
else
  vim.api.nvim_create_autocmd('VimEnter', { once = true, callback = function() vim.schedule(open) end })
end
