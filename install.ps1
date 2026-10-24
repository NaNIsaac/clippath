# clippath installer for Windows (Windows PowerShell 5.1 and PowerShell 7):
#   irm https://raw.githubusercontent.com/NaNIsaac/clippath/main/install.ps1 | iex
# Puts clippath.exe and clippathw.exe in $env:CLIPPATH_INSTALL_DIR (default
# %LOCALAPPDATA%\Programs\clippath) and adds that folder to the user PATH once.
# Pin a release with $env:CLIPPATH_VERSION = 'v0.5.0'. Run it again to update.
# Uninstall with $env:CLIPPATH_UNINSTALL = 1 (or true, yes; 0 and false leave it off) or .\install.ps1 -Uninstall; saved screenshots are kept.
param(
    [string]$Version = $env:CLIPPATH_VERSION,
    [string]$InstallDir = $env:CLIPPATH_INSTALL_DIR,
    [switch]$Uninstall = ($env:CLIPPATH_UNINSTALL -match '^(1|true|yes)$'),
    # Offline install from a release zip; its checksum must be in "<zip>.sha256" next to it.
    [string]$ZipPath = $env:CLIPPATH_ZIP
)
$ErrorActionPreference = 'Stop'
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

$repo = 'NaNIsaac/clippath'
$exes = 'clippath.exe', 'clippathw.exe'
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
if (-not $InstallDir) { $InstallDir = Join-Path $env:LOCALAPPDATA 'Programs\clippath' }
$InstallDir = [IO.Path]::GetFullPath($InstallDir).TrimEnd('\')

function Get-Sha256([string]$path) {
    # .NET instead of Get-FileHash, which is missing when Windows PowerShell inherits PowerShell 7's module path
    $fs = [IO.File]::OpenRead($path)
    try { -join ([Security.Cryptography.SHA256]::Create().ComputeHash($fs) | ForEach-Object { $_.ToString('x2') }) } finally { $fs.Dispose() }
}

function Test-SamePath([string]$a, [string]$b) {
    $a = [Environment]::ExpandEnvironmentVariables($a).TrimEnd('\')
    $a -ieq $b.TrimEnd('\')
}

# The exact release asset for this machine, never "the first asset".
function Select-ClippathAsset($release) {
    $arch = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
    # x64 builds also run on Windows on ARM through emulation; 32-bit Windows is not supported.
    if ($arch -notin 'AMD64', 'ARM64') { throw "clippath: unsupported architecture $arch (only 64-bit Windows)" }
    $name = "clippath-$($release.tag_name)-windows-x64.zip"
    $asset = @($release.assets | Where-Object { $_.name -ceq $name })
    if ($asset.Count -ne 1) { throw "clippath: release $($release.tag_name) has no $name" }
    $asset[0]
}

# The expected SHA-256 of an asset: the published .sha256 file if there is one, else GitHub's own digest.
function Get-ExpectedSha256($release, $asset) {
    $sidecar = @($release.assets | Where-Object { $_.name -ceq "$($asset.name).sha256" })
    if ($sidecar.Count -eq 1) {
        $file = [IO.Path]::GetTempFileName()
        try {
            # GitHub serves the .sha256 as binary, so read it from a file rather than from .Content
            Invoke-WebRequest -UseBasicParsing $sidecar[0].browser_download_url -OutFile $file
            return ((Get-Content -Raw $file).Trim() -split '\s+')[0].ToLowerInvariant()
        } finally { Remove-Item -Force $file -ErrorAction SilentlyContinue }
    }
    if ("$($asset.digest)" -match '^sha256:([0-9a-fA-F]{64})$') { return $Matches[1].ToLowerInvariant() }
    throw "clippath: no SHA-256 checksum published for $($asset.name); refusing to install"
}

function Get-UserPath { [Environment]::GetEnvironmentVariable('Path', 'User') }

function Add-UserPath([string]$dir) {
    $userPath = Get-UserPath
    if (-not (@("$userPath" -split ';' | Where-Object { $_ -and (Test-SamePath $_ $dir) }))) {
        [Environment]::SetEnvironmentVariable('Path', ("$userPath".TrimEnd(';') + ";$dir").TrimStart(';'), 'User')
        Write-Host "clippath: added $dir to your user PATH"
    }
    if (-not (@($env:Path -split ';' | Where-Object { $_ -and (Test-SamePath $_ $dir) }))) { $env:Path += ";$dir" }
}

function Remove-UserPath([string]$dir) {
    $userPath = Get-UserPath
    $kept = @("$userPath" -split ';' | Where-Object { $_ -and -not (Test-SamePath $_ $dir) })
    if ($kept.Count -ne @("$userPath" -split ';' | Where-Object { $_ }).Count) {
        [Environment]::SetEnvironmentVariable('Path', ($kept -join ';'), 'User')
        Write-Host "clippath: removed $dir from your user PATH"
    }
    $env:Path = (@($env:Path -split ';' | Where-Object { $_ -and -not (Test-SamePath $_ $dir) }) -join ';')
}

function Get-Autostart { (Get-ItemProperty $runKey -ErrorAction SilentlyContinue).clippath }

function Get-InstalledVersion {
    $exe = Join-Path $InstallDir 'clippath.exe'
    if (Test-Path $exe) { "$(& $exe --version 2>$null)".Trim() }
}

function Uninstall-Clippath {
    $exe = Join-Path $InstallDir 'clippath.exe'
    # stop closes the watcher and removes its login entry
    if (Test-Path $exe) { & $exe stop | Out-Null }
    elseif (Get-Autostart) { Remove-ItemProperty $runKey -Name clippath }
    # the private watcher copy made by `clippath start`; screenshots next to it are user data and stay
    $private = Join-Path $env:LOCALAPPDATA 'clippath\bin'
    Remove-Item -Force -Recurse $private -ErrorAction SilentlyContinue
    foreach ($name in $exes) {
        Remove-Item -Force (Join-Path $InstallDir $name), (Join-Path $InstallDir "$name.old") -ErrorAction SilentlyContinue
    }
    if ((Test-Path $InstallDir) -and -not (Get-ChildItem -Force $InstallDir)) { Remove-Item -Force $InstallDir }
    Remove-UserPath $InstallDir
    Write-Host "clippath: uninstalled from $InstallDir (screenshots in $(Join-Path $env:LOCALAPPDATA 'clippath') were kept)"
}

# Swaps both exes in together; on any failure the previous pair is put back.
function Install-Pair([string]$staged) {
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    $moved = @()
    $placed = @()
    try {
        foreach ($name in $exes) {
            $target = Join-Path $InstallDir $name
            # A running exe cannot be overwritten, but it can be renamed.
            Remove-Item -Force "$target.old" -ErrorAction SilentlyContinue
            if (Test-Path $target) { Move-Item -Force $target "$target.old"; $moved += $target }
        }
        foreach ($name in $exes) {
            $target = Join-Path $InstallDir $name
            Copy-Item -Force (Join-Path $staged $name) $target
            $placed += $target
        }
    } catch {
        foreach ($target in $placed) { Remove-Item -Force $target -ErrorAction SilentlyContinue }
        foreach ($target in $moved) { Move-Item -Force "$target.old" $target -ErrorAction SilentlyContinue }
        throw "clippath: install failed, previous version kept: $_"
    }
    # stays only while the old one runs
    foreach ($target in $moved) { Remove-Item -Force "$target.old" -ErrorAction SilentlyContinue }
}

function Install-Clippath {
    $work = Join-Path ([IO.Path]::GetTempPath()) ('clippath-' + [Guid]::NewGuid().ToString('n'))
    New-Item -ItemType Directory -Path $work | Out-Null
    try {
        if ($ZipPath) {
            $zip = [IO.Path]::GetFullPath($ZipPath)
            if (-not (Test-Path "$zip.sha256")) { throw "clippath: $zip.sha256 not found; refusing to install" }
            $want = ((Get-Content -Raw "$zip.sha256").Trim() -split '\s+')[0].ToLowerInvariant()
            $label = Split-Path -Leaf $zip
        } else {
            $api = "https://api.github.com/repos/$repo/releases/" + $(if ($Version -and $Version -ne 'latest') { "tags/$Version" } else { 'latest' })
            try { $release = Invoke-RestMethod -UseBasicParsing -Headers @{ 'User-Agent' = 'clippath-installer' } $api }
            catch { throw "clippath: cannot read release '$(if ($Version) { $Version } else { 'latest' })' from $api ($($_.Exception.Message))" }
            $asset = Select-ClippathAsset $release
            $want = Get-ExpectedSha256 $release $asset
            $zip = Join-Path $work $asset.name
            $label = $asset.name
            Write-Host "clippath: downloading $label"
            Invoke-WebRequest -UseBasicParsing $asset.browser_download_url -OutFile $zip
        }
        $got = Get-Sha256 $zip
        if ($want -ne $got) { throw "clippath: checksum mismatch for $label (expected $want, got $got); nothing was changed" }

        $unzipped = Join-Path $work 'unzipped'
        Expand-Archive -Path $zip -DestinationPath $unzipped
        $staged = @(Get-ChildItem -Recurse -File -Filter 'clippath.exe' $unzipped | Select-Object -ExpandProperty DirectoryName)
        if ($staged.Count -ne 1 -or -not (Test-Path (Join-Path $staged[0] 'clippathw.exe'))) {
            throw "clippath: $label does not contain clippath.exe and clippathw.exe together; nothing was changed"
        }
        $new = "$(& (Join-Path $staged[0] 'clippath.exe') --version)".Trim()
        if ($LASTEXITCODE -ne 0 -or $new -notmatch '^clippath ') { throw "clippath: the downloaded clippath.exe does not run; nothing was changed" }

        $old = Get-InstalledVersion
        $autostart = Get-Autostart
        Install-Pair $staged[0]
        Add-UserPath $InstallDir
        if ($old -and $old -ne $new) { Write-Host "clippath: updated $old -> $new in $InstallDir" }
        elseif ($old) { Write-Host "clippath: $new was already installed; files refreshed in $InstallDir" }
        else { Write-Host "clippath: installed $new to $InstallDir" }

        if ($autostart) {
            # refresh the watcher's private copy with the new build, keeping the --wsl choice
            $startArgs = @('start') + @(if ($autostart -match '--wsl') { '--wsl' })
            & (Join-Path $InstallDir 'clippath.exe') @startArgs | Out-Null
            if ($LASTEXITCODE -ne 0) { Write-Warning "clippath: could not restart the watcher; run 'clippath start' yourself" }
            else { Write-Host 'clippath: restarted the background watcher with the new version' }
        } else {
            Write-Host "clippath: open a new terminal, then run 'clippath start' once"
        }
        $others = @(Get-Command clippath -All -ErrorAction SilentlyContinue | Where-Object { $_.Source -and -not (Test-SamePath (Split-Path $_.Source) $InstallDir) })
        foreach ($other in $others) {
            Write-Warning "clippath: another clippath is on PATH at $($other.Source) (e.g. the old Python 0.4.0 package); remove it to avoid running the wrong one"
        }
    } finally {
        Remove-Item -Force -Recurse $work -ErrorAction SilentlyContinue
    }
}

# dot-sourcing the file only defines the functions (for the tests). A line that a terminal wrapper
# dot-sources and evaluates with iex also has InvocationName '.', but no script path.
if ($MyInvocation.InvocationName -eq '.' -and $PSCommandPath) { return }
if ($Uninstall) { Uninstall-Clippath } else { Install-Clippath }
