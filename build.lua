local nvim_version = ""
if vim.fn.has('nvim-0-10') then
  nvim_version = "neovim-0-10"
elseif vim.fn.has('nvim-0-9') then
  nvim_version = "neovim-0-9"
end

local os_uname = vim.loop.os_uname()
local os_name = string.lower(os_uname.sysname)
local machine = string.lower(os_uname.machine)

local artifact_name = "diffdirs-"..os_name.."-"..machine.."-"..nvim_version..".so"

vim.system({"curl", "-O", "diffdirs.so", "https://github.com/jayong93/diffdirs.nvim/release/latest/download/"..artifact_name},
  {text = true}):wait()
