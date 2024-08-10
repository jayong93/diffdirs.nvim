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
local download_url = "https://github.com/jayong93/diffdirs.nvim/releases/download/v0.2.2-alpha.1/" .. artifact_name

---@param cmd string
---@param opts table
---@return integer, string, string
local spawn_and_wait = function(cmd, opts)
  local stdout, stderr = vim.uv.new_pipe(), vim.uv.new_pipe()
  local out_str, err_str = "", ""

  local option = vim.tbl_deep_extend('keep', opts, { stdio = { nil, stdout, stderr } })
  local ret
  local process = vim.uv.spawn(cmd, option, function(code)
    ret = code
  end)
  stdout:read_start(function (err, data)
    if err ~= nil then
      err_str = err_str .. err
    end
    if data ~= nil then
      out_str = out_str .. data
    end
  end)
  stderr:read_start(function (err, data)
    if err ~= nil then
      err_str = err_str .. err
    end
    if data ~= nil then
      err_str = err_str .. data
    end
  end)

  if process ~= nil then
    repeat
      coroutine.yield()
    until (not process:is_active() and ret ~= nil)
  end

  stdout:close()
  stderr:close()
  return ret, out_str, err_str
end

spawn_and_wait("mkdir", { args = { "-p", plugin_dir .. "/lua" } })
coroutine.yield({ msg = "Try to download an artifact: " .. download_url, level = vim.log.levels.INFO })
local code = spawn_and_wait("curl",
  { args = { "--fail", "-L", "-o", plugin_dir .. "/lua/diffdirs.so", download_url } })
if code == 0 then
  return
end

coroutine.yield({ msg = "Failed to download an artifact, try to build locally", level = vim.log.levels.WARN })
local code, out, err = spawn_and_wait("cargo",
  { args = { "build", "--locked", "--release", "--features", nvim_version }, cwd = plugin_dir })
if code ~= 0 then
  coroutine.yield({ msg = '[OUT] '..out, level = vim.log.levels.INFO })
  coroutine.yield({ msg = '[ERR] '..err, level = vim.log.levels.WARN })
  coroutine.yield({ msg = "Failed to build locally with code: "..code , level = vim.log.levels.ERROR })
  return
end
local code, out, err = spawn_and_wait(vim.o.shell, {
  args = { vim.o.shellcmdflag,
    "mv target/release/libdiffdirs.dylib lua/diffdirs.so || mv target/release/libdiffdirs.so lua/diffdirs.so" },
  cwd = plugin_dir,
})
if code ~= 0 then
  coroutine.yield({ msg = '[OUT] '..out, level = vim.log.levels.INFO })
  coroutine.yield({ msg = '[ERR] '..err, level = vim.log.levels.WARN })
  coroutine.yield({ msg = "Failed to install with code: "..code, level = vim.log.levels.ERROR })
  return
end
coroutine.yield({msg="Done", level=vim.log.levels.TRACE})
