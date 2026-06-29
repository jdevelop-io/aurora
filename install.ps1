# Aurora installer for Windows (PowerShell).
#
#   irm https://raw.githubusercontent.com/jdevelop-io/aurora/main/install.ps1 | iex
#
# Recognized environment variables:
#   AURORA_VERSION       version to install (e.g. v0.2.0). Default: latest release.
#   AURORA_INSTALL_DIR   install directory. Default: $env:LOCALAPPDATA\aurora\bin.
#   AURORA_SKIP_CHECKSUM set to 1 to skip SHA-256 verification (not recommended;
#                        needed only for releases published before checksums).

$ErrorActionPreference = 'Stop'

$Repo = 'jdevelop-io/aurora'
$Bin  = 'aurora'

$InstallDir = if ($env:AURORA_INSTALL_DIR) { $env:AURORA_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'aurora\bin' }
$Version    = if ($env:AURORA_VERSION) { $env:AURORA_VERSION } else { 'latest' }

# --- architecture ----------------------------------------------------------
switch ($env:PROCESSOR_ARCHITECTURE) {
  'AMD64' { $target = 'x86_64-pc-windows-msvc' }
  'ARM64' {
    $target = 'x86_64-pc-windows-msvc'
    Write-Warning 'No native ARM64 build yet; installing the x64 binary (runs via emulation on Windows 11 ARM).'
  }
  default { throw "Unsupported architecture: $($env:PROCESSOR_ARCHITECTURE)" }
}

# --- version resolution ----------------------------------------------------
if ($Version -eq 'latest') {
  Write-Host '  Resolving the latest version...'
  $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" `
    -Headers @{ 'User-Agent' = 'aurora-installer' }
  $Version = $release.tag_name
  if (-not $Version) { throw 'Could not determine the latest version. Set AURORA_VERSION.' }
}

# --- download and install --------------------------------------------------
$asset = "$Bin-$Version-$target.zip"
$url   = "https://github.com/$Repo/releases/download/$Version/$asset"

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ([System.IO.Path]::GetRandomFileName())
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
  $zip = Join-Path $tmp $asset
  Write-Host "  Downloading $asset ($Version)..."
  Invoke-WebRequest -Uri $url -OutFile $zip

  # --- integrity check -----------------------------------------------------
  if ($env:AURORA_SKIP_CHECKSUM -eq '1') {
    Write-Warning 'Checksum verification skipped (AURORA_SKIP_CHECKSUM=1).'
  }
  else {
    Write-Host '  Verifying checksum...'
    try {
      $sumLine = (Invoke-WebRequest -Uri "$url.sha256" -Headers @{ 'User-Agent' = 'aurora-installer' } -UseBasicParsing).Content
    }
    catch {
      throw "Could not download the checksum ($url.sha256). This release may predate checksums; set AURORA_SKIP_CHECKSUM=1 to bypass at your own risk."
    }
    $expected = (($sumLine -split '\s+') | Where-Object { $_ })[0].ToLower()
    $actual   = (Get-FileHash -Algorithm SHA256 -Path $zip).Hash.ToLower()
    if ($expected -ne $actual) {
      throw "Checksum mismatch (expected $expected, got $actual). Aborting."
    }
    Write-Host '  Checksum OK.'
  }

  Expand-Archive -Path $zip -DestinationPath $tmp -Force
  $exe = Get-ChildItem -Path $tmp -Recurse -Filter "$Bin.exe" | Select-Object -First 1
  if (-not $exe) { throw "Binary '$Bin.exe' not found in the archive." }

  New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
  Copy-Item -Path $exe.FullName -Destination (Join-Path $InstallDir "$Bin.exe") -Force
  Write-Host "  Installed: $(Join-Path $InstallDir "$Bin.exe")"
}
finally {
  Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}

# --- add to user PATH ------------------------------------------------------
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$segments = ($userPath -split ';') | Where-Object { $_ }
if ($segments -notcontains $InstallDir) {
  $newPath = if ($userPath) { "$userPath;$InstallDir" } else { $InstallDir }
  [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
  $env:Path = "$env:Path;$InstallDir"
  Write-Host "  Added $InstallDir to your user PATH (restart your terminal to pick it up)."
}

Write-Host ''
Write-Host "aurora $Version is installed. Run: $Bin --help" -ForegroundColor Green
