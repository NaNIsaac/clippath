# Tests for install.ps1 against a real release zip, in Windows PowerShell 5.1 or PowerShell 7:
#   pwsh tests/install.ps1 -Zip clippath-v0.5.0-windows-x64.zip
# Uses a throwaway install folder and LOCALAPPDATA; the user PATH and the autostart entry are
# restored afterwards. Do not run it while your own clippath watcher is running: uninstall stops it.
param([Parameter(Mandatory)][string]$Zip)
$ErrorActionPreference = 'Stop'
$installer = Join-Path $PSScriptRoot '..\install.ps1'
$Zip = (Resolve-Path $Zip).Path
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$failures = 0

function Check([string]$name, [scriptblock]$test) {
    try { & $test; Write-Host "ok   $name" } catch { $script:failures++; Write-Host "FAIL $name`n     $_" }
}
function Assert($condition, [string]$message) { if (-not $condition) { throw $message } }
function Hash([string]$path) { (Get-Sha256 $path) }
function PathCount([string]$dir) { @([Environment]::GetEnvironmentVariable('Path', 'User') -split ';' | Where-Object { $_ -ieq $dir }).Count }
function Install([string]$zip) { & $installer -ZipPath $zip -InstallDir $dir *>&1 | Out-Null }

$root = Join-Path ([IO.Path]::GetTempPath()) ('clippath-test-' + [Guid]::NewGuid().ToString('n'))
New-Item -ItemType Directory $root | Out-Null
$savedPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$savedRun = (Get-ItemProperty $runKey -ErrorAction SilentlyContinue).clippath
$savedLocal = $env:LOCALAPPDATA
$env:LOCALAPPDATA = Join-Path $root 'local'
New-Item -ItemType Directory $env:LOCALAPPDATA | Out-Null
$dir = Join-Path $root 'Programs\clippath'
$sentinel = 'C:\clippath-test\keep-me'
[Environment]::SetEnvironmentVariable('Path', ("$savedPath".TrimEnd(';') + ";$sentinel").TrimStart(';'), 'User')

try {
    . $installer -InstallDir $dir

    # a copy of the zip with its .sha256, as the release workflow publishes it
    $good = Join-Path $root (Split-Path -Leaf $Zip)
    Copy-Item $Zip $good
    Set-Content -Encoding ascii "$good.sha256" "$(Hash $good)  $(Split-Path -Leaf $good)"

    Check 'selects the exact asset, whatever the order' {
        $release = [pscustomobject]@{ tag_name = 'v9.9.9'; assets = @(
            [pscustomobject]@{ name = 'clippath-v9.9.9-windows-x64.zip.sha256' },
            [pscustomobject]@{ name = 'install.ps1' },
            [pscustomobject]@{ name = 'clippath-v9.9.9-windows-x64.zip'; digest = 'sha256:' + ('ab' * 32) }) }
        Assert ((Select-ClippathAsset $release).name -eq 'clippath-v9.9.9-windows-x64.zip') 'wrong asset'
        $release.assets = @([pscustomobject]@{ name = 'clippath-v9.9.9-linux.zip' })
        $threw = $false
        try { Select-ClippathAsset $release } catch { $threw = $true }
        Assert $threw 'a release without the Windows zip must fail'
    }

    Check 'uses the GitHub digest when there is no .sha256 asset, and refuses without either' {
        $asset = [pscustomobject]@{ name = 'a.zip'; digest = 'sha256:' + ('AB' * 32) }
        Assert ((Get-ExpectedSha256 ([pscustomobject]@{ assets = @($asset) }) $asset) -eq ('ab' * 32)) 'digest not used'
        $asset.digest = $null
        $threw = $false
        try { Get-ExpectedSha256 ([pscustomobject]@{ assets = @($asset) }) $asset } catch { $threw = $true }
        Assert $threw 'missing checksum must fail'
    }

    Check 'fresh install puts both exes on PATH once' {
        Install $good
        Assert (Test-Path "$dir\clippath.exe") 'clippath.exe missing'
        Assert (Test-Path "$dir\clippathw.exe") 'clippathw.exe missing'
        Assert ((& "$dir\clippath.exe" --version) -match '^clippath \d') 'clippath --version failed'
        Assert ((PathCount $dir) -eq 1) "PATH has $dir $(PathCount $dir) times"
    }

    Check 'reinstall is idempotent and keeps other PATH entries' {
        Install $good
        [Environment]::SetEnvironmentVariable('Path', [Environment]::GetEnvironmentVariable('Path', 'User') + ";$dir\", 'User')
        Install $good
        $count = @([Environment]::GetEnvironmentVariable('Path', 'User') -split ';' | Where-Object { $_.TrimEnd('\') -ieq $dir }).Count
        Assert ($count -eq 2) "a second run added $dir again ($count entries)"
        Assert ((PathCount $sentinel) -eq 1) 'unrelated PATH entry lost'
        Assert (-not (Get-ChildItem "$dir\*.old")) 'leftover .old files'
    }

    $before = Hash "$dir\clippath.exe"

    Check 'checksum mismatch changes nothing' {
        $bad = Join-Path $root 'bad.zip'
        Copy-Item $good $bad
        Set-Content -Encoding ascii "$bad.sha256" ('0' * 64)
        $threw = $false
        try { Install $bad } catch { $threw = $_ -match 'checksum mismatch' }
        Assert $threw 'mismatch was not reported'
        Assert ((Hash "$dir\clippath.exe") -eq $before) 'installed exe changed'
    }

    Check 'a zip without clippathw.exe changes nothing' {
        $part = Join-Path $root 'part'
        Expand-Archive $good $part
        Get-ChildItem -Recurse "$part\clippathw.exe" | Remove-Item
        $half = Join-Path $root 'half.zip'
        Compress-Archive "$part\*" $half
        Set-Content -Encoding ascii "$half.sha256" (Hash $half)
        $threw = $false
        try { Install $half } catch { $threw = $_ -match 'together' }
        Assert $threw 'incomplete zip was not reported'
        Assert ((Hash "$dir\clippath.exe") -eq $before) 'installed exe changed'
        Assert (Test-Path "$dir\clippathw.exe") 'clippathw.exe was removed'
    }

    Check 'a missing .sha256 is refused' {
        $bare = Join-Path $root 'bare.zip'
        Copy-Item $good $bare
        $threw = $false
        try { Install $bare } catch { $threw = $_ -match 'sha256 not found' }
        Assert $threw 'install without checksum was not refused'
    }

    Check 'uninstall removes exes, PATH entries and autostart but keeps screenshots' {
        $data = Join-Path $env:LOCALAPPDATA 'clippath'
        New-Item -ItemType Directory -Force "$data\bin" | Out-Null
        Set-Content "$data\bin\clippathw.exe" 'stale copy'
        Set-Content "$data\clip-test.png" 'user screenshot'
        Set-ItemProperty $runKey -Name clippath -Value "`"$data\bin\clippathw.exe`""
        & $installer -Uninstall -InstallDir $dir *>&1 | Out-Null
        Assert (-not (Test-Path $dir)) 'install folder left behind'
        Assert (-not (Test-Path "$data\bin")) 'private watcher copy left behind'
        Assert (Test-Path "$data\clip-test.png") 'screenshot was deleted'
        Assert ($null -eq (Get-ItemProperty $runKey -ErrorAction SilentlyContinue).clippath) 'autostart entry left behind'
        $left = @([Environment]::GetEnvironmentVariable('Path', 'User') -split ';' | Where-Object { $_.TrimEnd('\') -ieq $dir }).Count
        Assert ($left -eq 0) 'PATH entry left behind'
        Assert ((PathCount $sentinel) -eq 1) 'unrelated PATH entry lost'
    }

    Check 'uninstall twice is harmless' {
        & $installer -Uninstall -InstallDir $dir *>&1 | Out-Null
        Assert ((PathCount $sentinel) -eq 1) 'unrelated PATH entry lost'
    }
} finally {
    [Environment]::SetEnvironmentVariable('Path', $savedPath, 'User')
    if ($savedRun) { Set-ItemProperty $runKey -Name clippath -Value $savedRun }
    else { Remove-ItemProperty $runKey -Name clippath -ErrorAction SilentlyContinue }
    $env:LOCALAPPDATA = $savedLocal
    Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
}

Write-Host "PowerShell $($PSVersionTable.PSVersion): $failures failure(s)"
exit [int]($failures -gt 0)
