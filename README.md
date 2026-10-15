<img src="assets/logo.png" width="96" align="right" alt="">

# clippath

Screenshot once. Paste where you need it.

Turns the image on your Windows clipboard into a PNG file and puts its path on the
clipboard, so you can paste screenshots into terminal AI agents (Claude Code, Codex, ...)
on Windows or WSL. The image stays on the clipboard too, so pasting into Paint or a
messenger still gives you the image. Python stdlib only, no dependencies.

## Install

Recommended (puts `clippath` on PATH for you):

```powershell
uv tool install .
uv tool update-shell  # first time only, then open a new terminal
```

No uv yet? `powershell -c "irm https://astral.sh/uv/install.ps1 | iex"`
(`pipx install .` + `pipx ensurepath` also works.)

With plain pip:

```powershell
pip install .
```

If pip warns `clippath.exe is installed in '...\Scripts' which is not on PATH`,
add that folder to your user PATH once, then open a new terminal:

```powershell
$d = python -c "import sysconfig; print(sysconfig.get_path('scripts'))"
[Environment]::SetEnvironmentVariable("Path", [Environment]::GetEnvironmentVariable("Path","User") + ";$d", "User")
```

No PATH change at all? This always works:

```powershell
python -m clippath copy
```

## Use

Once:

```powershell
clippath start          # watch in background, also at every login
clippath start --wsl    # same, but copy /mnt/c/... paths
```

Then just screenshot (`Win+Shift+S`, PrintScreen, any tool) and `Ctrl+V` in your
AI terminal: the path is pasted. Paint or a messenger still gets the image.
Normal text copies are left alone.

While it runs, the clippath icon sits in the tray (click it: open folder, quit) and
Task Manager lists it as `clippath.exe`.

## Commands

| Command | What it does |
|---|---|
| `clippath start [--wsl]` | Watch in background, also at every login |
| `clippath stop` | Stop the watcher and remove it from login |
| `clippath status` | Watcher, autostart and saved-file info |
| `clippath watch [--wsl]` | Watch in the foreground (Ctrl+C to stop) |
| `clippath copy [--wsl]` | Convert the clipboard image once |
| `clippath clean [--days N]` | Delete saved screenshots (keep the last N days) |

`clippath -h`, `clippath <command> -h`, `clippath --version`.

Manual, one shot:

1. Take a screenshot to the clipboard.
2. Run in PowerShell:

   ```powershell
   clippath copy          # copies C:\Users\...\clippath\<hash>.png
   clippath copy --wsl    # copies /mnt/c/Users/.../clippath/<hash>.png
   ```

3. `Ctrl+V` in your AI terminal.

If your screenshot tool already saved the file (Snipping Tool, `Win+PrtScn` →
`Pictures\Screenshots`), clippath copies that file's path instead of making a second
copy. Other images are saved in `%LOCALAPPDATA%\clippath\`, named by SHA-256, so the
same image is saved once; `clippath clean` only touches this folder. Nothing is sent
anywhere.
