local scope = vim.json.decode(vim.env.TSUMUGI_REVIEW)
vim.env.TSUMUGI_REVIEW = nil

local function open()
  local rev = scope.Files or (scope.Diff and scope.Diff[1]) or 'WorkingCopy'
  local change = scope.Diff and scope.Diff[2]
  local commit = type(rev) == 'table' and rev.Commit or nil
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
    local range = commit and (commit .. '^!') or 'HEAD'
    if git({ 'rev-parse', '--verify', commit and (commit .. '^') or 'HEAD' }) then
      local args = { range }
      if change then
        vim.list_extend(args, { '--', change.path })
        if change.from and change.from ~= vim.NIL then table.insert(args, change.from) end
      end
      local ok, err = pcall(diffview.open, args)
      if ok then return end
      vim.notify('DiffviewOpen failed: ' .. tostring(err) .. '. Opening built-in diff.', vim.log.levels.WARN)
    end
  end
  local base = commit and (commit .. '^') or 'HEAD'
  local files
  if change then
    files = { change.path }
  elseif commit then
    files = paths(git({ 'diff-tree', '--root', '--no-commit-id', '--no-renames', '-r', '--name-only', '-z', commit, '--' }))
  else
    files = paths(git({ 'diff', '--no-ext-diff', '--no-textconv', '--no-renames', '--name-only', '-z', 'HEAD', '--' }))
    if not git({ 'rev-parse', '--verify', 'HEAD' }) then
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
    end
  end
  if first then vim.notify('No changes', vim.log.levels.INFO) end
end

if vim.v.vim_did_enter == 1 then
  open()
else
  vim.api.nvim_create_autocmd('VimEnter', { once = true, callback = function() vim.schedule(open) end })
end
