<img src="assets/logo.png" width="96" align="right" alt="">

# clippath

**Screenshot once. Paste where you need it.**

[한국어](README.ko.md)

Turns the image on your Windows clipboard into a PNG file and puts its path on the
clipboard, so you can paste screenshots into terminal AI agents (Claude Code, Codex, ...)
on Windows or WSL. The image stays on the clipboard too, so pasting into Paint or a
messenger still gives you the image.

![Screenshot a pricing card, paste it into Claude Code, get the card back as HTML](assets/claude-code.gif)

![Screenshot, clippath adds the file path to the clipboard, Ctrl+V gives the path in a terminal and the image in other apps](assets/how-it-works.svg)

## Install

In PowerShell (Windows PowerShell 5.1 or PowerShell 7):

<!-- test windows -->
```powershell
irm https://raw.githubusercontent.com/NaNIsaac/clippath/main/install.ps1 | iex
```

[`install.ps1`](install.ps1) picks the `clippath-<version>-windows-x64.zip` asset by its exact name,
checks its SHA-256 (the release's `.sha256` file, or GitHub's asset digest for older releases),
puts `clippath.exe` and `clippathw.exe` in `%LOCALAPPDATA%\Programs\clippath` and adds that folder
to your user PATH once. If the download or the checksum fails, an existing install is left as it was.

- **Update**: see [Update](#update).
- **Pin a version**: `$env:CLIPPATH_VERSION = 'v0.5.0'` first.
- **Other folder**: `$env:CLIPPATH_INSTALL_DIR = 'D:\Tools\clippath'` first.
- **Uninstall**: `& ([scriptblock]::Create((irm https://raw.githubusercontent.com/NaNIsaac/clippath/main/install.ps1))) -Uninstall`.
  This stops the watcher, removes it from login and from PATH, and deletes the exes. Saved screenshots in `%LOCALAPPDATA%\clippath` are kept;
  `clippath clean` before uninstalling deletes them.

You can [read the script](install.ps1) before running it, or download the zip from the
[latest release](https://github.com/NaNIsaac/clippath/releases/latest) and put both exes in one
folder on your PATH yourself.

Open a new terminal afterwards. `clippath.exe` is the command; `clippathw.exe` is the same watcher
without a console window, which `clippath start` runs at login. Keep them together.

Check the install:

<!-- test windows -->
```powershell
clippath --version
```

## Update

Run the install line again. It downloads the latest release, checks its SHA-256, replaces both exes in
`%LOCALAPPDATA%\Programs\clippath` and restarts a running background watcher on the new version. It
prints what it did, for example `clippath: updated 0.5.0 -> 0.6.0`. If it prints nothing, it did not run.

<!-- test windows -->
```powershell
irm https://raw.githubusercontent.com/NaNIsaac/clippath/main/install.ps1 | iex
```

Open a new terminal and check the version:

<!-- test windows -->
```powershell
clippath --version
```

If it still shows 0.4.0, a copy from the old Python package comes first on PATH (the installer warns
about it). Remove that copy, open a new terminal, and check which `clippath` runs first:

```powershell
uv tool uninstall clippath    # or: pip uninstall clippath
Get-Command clippath -All     # the first line must end in Programs\clippath\clippath.exe
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

While it runs, the clippath icon sits in the tray (click it: open folder, quit).

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
in `%LOCALAPPDATA%\clippath\`, named by SHA-256, so the same image is saved once;
`clippath clean` only touches this folder. Nothing is sent anywhere.

## Building from source

```bash
cargo build --release   # target\release\clippath.exe and clippathw.exe
cargo test
```
