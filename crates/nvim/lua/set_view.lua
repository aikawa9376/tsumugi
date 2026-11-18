local view = ...
-- The window of tsumugi's own buffer, shown again if `:e` or `:help` left it.
local b = vim.g.tsumugi_buf
if b and vim.api.nvim_buf_is_valid(b) and vim.api.nvim_get_current_buf() ~= b then
  vim.api.nvim_win_set_buf(0, b)
end
vim.fn.winrestview(view)
if vim.g.tsumugi_view then
  vim.g.tsumugi_view()
end
