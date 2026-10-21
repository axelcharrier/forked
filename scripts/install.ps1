# Installs Forked for the current user on Windows, with Start menu and desktop
# shortcuts. No administrator rights needed.
#
#   powershell -ExecutionPolicy Bypass -File scripts\install.ps1
#
# From a clone of the repository it compiles Forked first. From the archive
# made by scripts/build-windows.sh (Forked.exe next to this script) it installs
# that executable directly, without Rust.
param(
    [switch]$NoDesktopShortcut
)
$ErrorActionPreference = "Stop"

$Dest = if ($env:FORKED_INSTALL_DIR) { $env:FORKED_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "Programs\Forked" }
$Prebuilt = Join-Path $PSScriptRoot "Forked.exe"

if (Test-Path $Prebuilt) {
    $Exe = $Prebuilt
    $Icon = Join-Path $PSScriptRoot "forked.ico"
} else {
    $Root = Split-Path -Parent $PSScriptRoot
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        $CargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
        if (Test-Path (Join-Path $CargoBin "cargo.exe")) {
            $env:Path = "$CargoBin;$env:Path"
        } else {
            throw "Rust est introuvable. Installez-le depuis https://rustup.rs puis relancez ce script."
        }
    }
    Write-Host "==> Compilation (release)"
    cargo build --release --manifest-path (Join-Path $Root "Cargo.toml")
    if ($LASTEXITCODE -ne 0) { throw "La compilation a échoué." }
    $Exe = Join-Path $Root "target\release\forked.exe"
    $Icon = Join-Path $Root "assets\icon\forked.ico"
}

if (-not (Get-Command git -ErrorAction SilentlyContinue) -and
    -not (Test-Path "C:\Program Files\Git\cmd\git.exe")) {
    Write-Warning "Git est introuvable. Forked en a besoin : https://git-scm.com/download/win"
}

Write-Host "==> Installation dans $Dest"
# Ask a running Forked to close so its executable can be replaced.
$Running = Get-Process forked -ErrorAction SilentlyContinue
if ($Running) {
    $Running | ForEach-Object { $_.CloseMainWindow() | Out-Null }
    $Running | Wait-Process -Timeout 5 -ErrorAction SilentlyContinue
}
New-Item -ItemType Directory -Force -Path $Dest | Out-Null
Copy-Item $Exe (Join-Path $Dest "Forked.exe") -Force
if (Test-Path $Icon) { Copy-Item $Icon (Join-Path $Dest "forked.ico") -Force }

$Shell = New-Object -ComObject WScript.Shell
function New-Shortcut([string]$Path) {
    $Link = $Shell.CreateShortcut($Path)
    $Link.TargetPath = Join-Path $Dest "Forked.exe"
    $Link.WorkingDirectory = $Dest
    $Link.IconLocation = Join-Path $Dest "forked.ico"
    $Link.Description = "Forked - client Git"
    $Link.Save()
}
New-Shortcut (Join-Path ([Environment]::GetFolderPath("Programs")) "Forked.lnk")
if (-not $NoDesktopShortcut) {
    New-Shortcut (Join-Path ([Environment]::GetFolderPath("Desktop")) "Forked.lnk")
}

Write-Host "Forked est installé : $Dest\Forked.exe"
Write-Host "Lancez-le depuis le menu Démarrer (tapez « Forked ») ou le raccourci du Bureau."
