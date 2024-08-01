local nvim_version = ""
if vim.fn.has('nvim-0-10') then
  nvim_version = "neovim-0-10"
elseif vim.fn.has('nvim-0-9') then
  nvim_version = "neovim-0-9"
else
  nvim_version = "neovim-nightly"
end

local os_uname = vim.loop.os_uname()
local os_name = string.lower(os_uname.sysname)
local machine = string.lower(os_uname.machine)

local artifact_name = "diffdirs-" .. os_name .. "-" .. machine .. "-" .. nvim_version .. ".so"

local plugin_dir = vim.fn.fnamemodify(debug.getinfo(1, "S").source:sub(2), ":p:h")
local download_url = "https://github.com/jayong93/diffdirs.nvim/releases/download/v0.2.1" .. artifact_name

local build_local = function()
  vim.system({"cargo", "build", "--locked", "--release", "--features", nvim_version}, {cwd=plugin_dir, text=true}):wait()
  vim.system({vim.o.shell, vim.o.shellcmdflag,
    "mv target/release/libdiffdirs.dylib lua/diffdirs.so || mv target/release/libdiffdirs.so lua/diffdirs.so"},
    {cwd=plugin_dir, text=true}):wait()
end

coroutine.yield({msg="test", level=vim.log.levels.WARN})

-- vim.notify_once("Try to download an artifact: " .. download_url, vim.log.levels.TRACE)
vim.system({ "mkdir", plugin_dir .. "/lua" }, { text = true }):wait()
local r = vim.system({ "curl", "--fail", "-L", "-o", plugin_dir .. "/lua/diffdirs.so", download_url },
  { text = true }):wait()
if r.code ~= 0 then
  build_local()
end
