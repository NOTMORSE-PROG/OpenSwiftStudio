# Build OpenSwiftStudio in release mode, copy the standalone .exe to a stable
# user-local path, and drop (or refresh) a Desktop shortcut. Re-run after each
# code change to refresh your local install.
#
#   Output exe:  %LOCALAPPDATA%\OpenSwiftStudio-dev\openswiftstudio.exe
#   Shortcut:    %USERPROFILE%\Desktop\OpenSwiftStudio (dev).lnk
#
# Usage (from the repo root or anywhere):
#   powershell -ExecutionPolicy Bypass -File scripts\install-dev.ps1
#
# The script is idempotent — safe to re-run. It will:
#   - Refuse to run if the app is currently launched (the .exe would be locked)
#   - Rebuild via `npm run tauri:build -- --no-bundle` (skips MSI/NSIS bundles)
#   - Overwrite the installed .exe in place
#   - Re-write the Desktop shortcut (no-op if unchanged)

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

$installDir = Join-Path $env:LOCALAPPDATA 'OpenSwiftStudio-dev'
$installExe = Join-Path $installDir 'openswiftstudio.exe'
$shortcut   = Join-Path ([Environment]::GetFolderPath('Desktop')) 'OpenSwiftStudio (dev).lnk'

# Fail fast if the app is open — the .exe will be locked and Copy-Item would
# error out mid-script with a less-helpful message.
$running = Get-Process -Name 'openswiftstudio' -ErrorAction SilentlyContinue
if ($running) {
    $pids = ($running | ForEach-Object { $_.Id }) -join ', '
    Write-Host "[install-dev] OpenSwiftStudio is currently running (PID $pids)." -ForegroundColor Yellow
    Write-Host "[install-dev] Close it first, then re-run this script." -ForegroundColor Yellow
    exit 1
}

Write-Host "[install-dev] Building (release, no bundle)..." -ForegroundColor Cyan
& npm run tauri:build -- --no-bundle
if ($LASTEXITCODE -ne 0) { throw "tauri build failed (exit $LASTEXITCODE)" }

$builtExe = Join-Path $repoRoot 'src-tauri\target\release\openswiftstudio.exe'
if (-not (Test-Path $builtExe)) { throw "Build did not produce $builtExe" }

New-Item -ItemType Directory -Force -Path $installDir | Out-Null
Write-Host "[install-dev] Copying to $installExe" -ForegroundColor Cyan
Copy-Item -Force $builtExe $installExe

$wsh = New-Object -ComObject WScript.Shell
$sc  = $wsh.CreateShortcut($shortcut)
$sc.TargetPath       = $installExe
$sc.WorkingDirectory = $installDir
$sc.IconLocation     = "$installExe,0"
$sc.Description      = 'OpenSwiftStudio (dev build)'
$sc.Save()

$sizeMb = [math]::Round((Get-Item $installExe).Length / 1MB, 2)
Write-Host "[install-dev] Done." -ForegroundColor Green
Write-Host "[install-dev]   exe:      $installExe ($sizeMb MB)" -ForegroundColor Green
Write-Host "[install-dev]   shortcut: $shortcut" -ForegroundColor Green
Write-Host "[install-dev] Launch via the 'OpenSwiftStudio (dev)' Desktop shortcut." -ForegroundColor Green
