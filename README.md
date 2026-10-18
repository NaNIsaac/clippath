<img src="assets/logo.png" width="96" align="right" alt="">

# clippath

**Screenshot once. Paste where you need it.**

[한국어](README.ko.md)

Turns the image on your Windows clipboard into a PNG file and puts its path on the
clipboard, so you can paste screenshots into terminal AI agents (Claude Code, Codex, ...)
on Windows or WSL. The image stays on the clipboard too, so pasting into Paint or a
messenger still gives you the image. Python stdlib only, no dependencies.

![Screenshot a pricing card, paste it into Claude Code, get the card back as HTML](assets/claude-code.gif)

![Screenshot, clippath adds the file path to the clipboard, Ctrl+V gives the path in a terminal and the image in other apps](assets/how-it-works.svg)

## Install

Recommended (puts `clippath` on PATH for you):

```powershell
uv tool install git+https://github.com/NaNIsaac/clippath
uv tool update-shell  # first time only, then open a new terminal
```

No uv yet? `powershell -c "irm https://astral.sh/uv/install.ps1 | iex"`
(`pipx install git+https://github.com/NaNIsaac/clippath` + `pipx ensurepath` also works.)

With plain pip:

```powershell
pip install git+https://github.com/NaNIsaac/clippath
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
AI terminal:

```text
> what's wrong with this layout? "C:\Users\me\Pictures\Screenshots\Screenshot 2026-10-16 091500.png"
```

The path is quoted, so spaces are safe. Only terminals, code editors, Notepad and
Explorer get the path; browsers (ChatGPT, ...), messengers and Paint get just the image.
Normal text copies are left alone.

It is a plain path, so any command can use it:

![Screenshot, then Ctrl+V in PowerShell pastes the quoted file path into a cp command](assets/terminal.gif)

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

## Where files go

If your screenshot tool already saved the file (Snipping Tool, `Win+PrtScn` →
`Pictures\Screenshots`), clippath copies that file's path instead of making a second
copy, after checking that its pixels match the clipboard image. Other images are saved
in `%LOCALAPPDATA%\clippath\`, named by SHA-256, so the same image is saved once; `clippath clean` only touches this folder. Nothing is sent
anywhere.
