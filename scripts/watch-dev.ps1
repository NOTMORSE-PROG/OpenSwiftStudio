# Watch the repo for source changes and auto-rebuild + reinstall + relaunch.
#
# Loop per change:
#   1. Wait for the filesystem to settle (debounce, default 800 ms)
#   2. Kill the running OpenSwiftStudio if any (so the .exe isn't locked)
#   3. Run scripts\install-dev.ps1 (release build, copy to %LOCALAPPDATA%)
#   4. Relaunch via the Desktop shortcut
#
# Stop with Ctrl+C.
#
# When to use this vs. the alternatives:
#   - Iterating on frontend code (instant hot-reload):  npm run tauri:dev
#   - Verifying a real release-build install:           this script
#   - One-off update after a single change:             scripts\install-dev.ps1
#
# A pure-frontend change rebuilds in ~5-10 s (Rust binary unchanged → cargo
# skips). A Rust change costs ~10-90 s for the incremental compile.

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

$installScript = Join-Path $PSScriptRoot 'install-dev.ps1'
$shortcut      = Join-Path ([Environment]::GetFolderPath('Desktop')) 'OpenSwiftStudio (dev).lnk'

# Directories whose changes should trigger a rebuild. Recursive. Anything not
# listed (target/, node_modules/, dist/, etc.) is ignored.
$watchedDirs = @(
    'src',
    'src-tauri\src',
    'src-tauri\capabilities',
    'src-tauri\icons'
) | ForEach-Object { Join-Path $repoRoot $_ } | Where-Object { Test-Path $_ }

# Single-file watches (a directory watcher with filter would over-match).
$watchedFiles = @(
    'index.html',
    'package.json',
    'src-tauri\Cargo.toml',
    'src-tauri\tauri.conf.json'
) | ForEach-Object { Join-Path $repoRoot $_ } | Where-Object { Test-Path $_ }

$debounceMs = 800

# Shared state for handlers and the main loop.
$state = [hashtable]::Synchronized(@{
    LastEvent = [DateTime]::MinValue
    Pending   = $false
})

$watchers = @()
$subs     = @()

function Register-Watch {
    param([string]$Path, [bool]$IsFile)
    $w = New-Object System.IO.FileSystemWatcher
    if ($IsFile) {
        $w.Path   = Split-Path -Parent $Path
        $w.Filter = Split-Path -Leaf  $Path
        $w.IncludeSubdirectories = $false
    } else {
        $w.Path   = $Path
        $w.IncludeSubdirectories = $true
    }
    $w.NotifyFilter = [System.IO.NotifyFilters]::LastWrite -bor `
                      [System.IO.NotifyFilters]::FileName  -bor `
                      [System.IO.NotifyFilters]::DirectoryName
    $w.EnableRaisingEvents = $true
    $script:watchers += $w

    $action = {
        $now = [DateTime]::Now
        $event.MessageData.LastEvent = $now
        $event.MessageData.Pending   = $true
    }
    foreach ($evt in @('Changed','Created','Deleted','Renamed')) {
        $script:subs += Register-ObjectEvent -InputObject $w -EventName $evt -Action $action -MessageData $state
    }
}

foreach ($d in $watchedDirs)  { Register-Watch -Path $d -IsFile:$false }
foreach ($f in $watchedFiles) { Register-Watch -Path $f -IsFile:$true  }

Write-Host "[watch-dev] Watching $($watchedDirs.Count) dirs + $($watchedFiles.Count) files. Debounce: $debounceMs ms." -ForegroundColor Cyan
Write-Host "[watch-dev] Save any source file to trigger a rebuild. Ctrl+C to stop." -ForegroundColor Cyan

try {
    while ($true) {
        Start-Sleep -Milliseconds 150

        if (-not $state.Pending) { continue }
        $sinceLast = ([DateTime]::Now - $state.LastEvent).TotalMilliseconds
        if ($sinceLast -lt $debounceMs) { continue }

        $state.Pending = $false
        Write-Host ""
        Write-Host "[watch-dev] Change detected. Rebuilding..." -ForegroundColor Yellow

        # Kill the running app so install-dev.ps1 can overwrite the .exe.
        $running = Get-Process -Name 'openswiftstudio' -ErrorAction SilentlyContinue
        if ($running) {
            Write-Host "[watch-dev] Stopping running instance (PID $($running.Id -join ', '))..." -ForegroundColor Yellow
            $running | Stop-Process -Force
            Start-Sleep -Milliseconds 500
        }

        # Reset the debounce flag *before* the build so saves during the build
        # accumulate into the next cycle instead of being lost.
        $state.LastEvent = [DateTime]::Now
        $state.Pending   = $false

        & powershell -ExecutionPolicy Bypass -File $installScript
        if ($LASTEXITCODE -ne 0) {
            Write-Host "[watch-dev] Build failed. Fix the error and save again." -ForegroundColor Red
            continue
        }

        if (Test-Path $shortcut) {
            Start-Process -FilePath $shortcut
            Write-Host "[watch-dev] Relaunched. Watching for next change..." -ForegroundColor Green
        } else {
            Write-Host "[watch-dev] Shortcut missing at $shortcut — run install-dev.ps1 once first." -ForegroundColor Red
        }
    }
}
finally {
    Write-Host "[watch-dev] Cleaning up watchers..." -ForegroundColor Cyan
    foreach ($s in $subs)    { Unregister-Event -SubscriptionId $s.Id -ErrorAction SilentlyContinue; Remove-Job -Id $s.Id -Force -ErrorAction SilentlyContinue }
    foreach ($w in $watchers) { $w.EnableRaisingEvents = $false; $w.Dispose() }
}
