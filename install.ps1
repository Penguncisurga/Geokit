# GeoKit Windows One-Line Installer (with SHA256 verification & safe daemon stop)
# Usage: irm https://raw.githubusercontent.com/Penguncisurga/Geokit/main/install.ps1 | iex

$ErrorActionPreference = "Stop"

$InstallDir = Join-Path $env:USERPROFILE ".geokit\bin"
$ExePath = Join-Path $InstallDir "geokit.exe"
$TmpExe = Join-Path $InstallDir "geokit.exe.part"
$SumsPath = Join-Path $InstallDir "SHA256SUMS.txt"
$AssetName = "geokit-windows-x86_64.exe"
$BaseUrl = "https://github.com/Penguncisurga/Geokit/releases/latest/download"
$DownloadUrl = "$BaseUrl/$AssetName"
$SumsUrl = "$BaseUrl/SHA256SUMS.txt"

Write-Host "Installing geokit..." -ForegroundColor Cyan
New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null

# Stop running warm daemons cleanly before replacing geokit.exe
if (Test-Path $ExePath) {
    try { & $ExePath daemon stop 2>$null | Out-Null } catch {}
}

Write-Host "Downloading latest release from $DownloadUrl"
Invoke-WebRequest -Uri $DownloadUrl -OutFile $TmpExe -UseBasicParsing

# Verify SHA256 checksum against published SHA256SUMS.txt
try {
    Invoke-WebRequest -Uri $SumsUrl -OutFile $SumsPath -UseBasicParsing
    $ExpectedLine = Get-Content $SumsPath | Where-Object { $_ -match "\b$([regex]::Escape($AssetName))$" } | Select-Object -First 1
    if ($ExpectedLine) {
        $ExpectedHash = ($ExpectedLine -split "\s+")[0].ToLowerInvariant()
        $ActualHash = (Get-FileHash -Algorithm SHA256 -Path $TmpExe).Hash.ToLowerInvariant()
        if ($ActualHash -ne $ExpectedHash) {
            Remove-Item $TmpExe -Force -ErrorAction SilentlyContinue
            throw "SHA256 mismatch for $AssetName! Expected $ExpectedHash, got $ActualHash"
        }
        Write-Host "[OK] SHA256 checksum verified ($ActualHash)" -ForegroundColor Green
    }
    Remove-Item $SumsPath -Force -ErrorAction SilentlyContinue
} catch {
    if ($_.Exception.Message -like "*SHA256 mismatch*") { throw }
    Write-Host "[WARN] Could not verify SHA256SUMS.txt ($($_.Exception.Message))" -ForegroundColor Yellow
}

Move-Item -Path $TmpExe -Destination $ExePath -Force

# Add to User PATH if not already present
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if (-not $UserPath) { $UserPath = "" }
$PathParts = $UserPath -split ";" | Where-Object { $_ -ne "" }
if ($PathParts -notcontains $InstallDir) {
    $NewPath = ($PathParts + $InstallDir) -join ";"
    [Environment]::SetEnvironmentVariable("Path", $NewPath, "User")
    Write-Host "[OK] Added $InstallDir to your User PATH." -ForegroundColor Green
}

$env:PATH = "$InstallDir;$env:PATH"
Write-Host "[OK] Installed geokit -> $ExePath" -ForegroundColor Green
Write-Host ""
& $ExePath --version
& $ExePath doctor
Write-Host "Tip: If you use ArcMap 10.1-10.8, close ArcMap and run: geokit live install-addin" -ForegroundColor Yellow
Write-Host "Restart your terminal to use `geokit` anywhere." -ForegroundColor Cyan
