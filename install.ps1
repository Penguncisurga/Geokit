# GeoKit Windows One-Line Installer
# Usage: irm https://raw.githubusercontent.com/Penguncisurga/Geokit/main/install.ps1 | iex

$ErrorActionPreference = "Stop"

$InstallDir = Join-Path $env:USERPROFILE ".geokit\bin"
$ExePath = Join-Path $InstallDir "geokit.exe"
$DownloadUrl = "https://github.com/Penguncisurga/Geokit/releases/latest/download/geokit-windows-x86_64.exe"

Write-Host "Installing geokit..." -ForegroundColor Cyan
New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null

Write-Host "Downloading latest release from $DownloadUrl"
Invoke-WebRequest -Uri $DownloadUrl -OutFile $ExePath -UseBasicParsing

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
