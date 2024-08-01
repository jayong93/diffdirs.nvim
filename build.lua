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

local plugin_dir = vim.fn.fnamemodify(debug.getinfo(1, "S").source:sub(2), ":p:h")
local download_url = "https://github.com/jayong93/diffdirs.nvim/releases/latest/download/"..artifact_name
vim.system({"mkdir", plugin_dir.."/lua"}, {text = true}):wait()
vim.system({"curl", "-L", "-o", plugin_dir.."/lua/diffdirs.so", download_url}, {text = true}):wait()
