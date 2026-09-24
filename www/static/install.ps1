# Installs the latest cydonia release on Windows (x86_64).
#
#   irm https://cydonia.sh/install.ps1 | iex
#
# Runs the release's installer silently: per-user, into
# %LOCALAPPDATA%\Programs\cydonia, with a Start menu shortcut and an entry in
# Installed apps.
$ErrorActionPreference = 'Stop'

$url = 'https://github.com/crabtalk/cydonia/releases/latest/download/cydonia-windows-x86_64-setup.exe'
$setup = Join-Path ([IO.Path]::GetTempPath()) "cydonia-$([guid]::NewGuid())-setup.exe"

Write-Host "downloading $url"
Invoke-WebRequest -Uri $url -OutFile $setup -UseBasicParsing
try {
    $run = Start-Process -FilePath $setup -Wait -PassThru `
        -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/CLOSEAPPLICATIONS'
    if ($run.ExitCode -ne 0) {
        throw "the installer exited with code $($run.ExitCode)"
    }
} finally {
    Remove-Item $setup -ErrorAction SilentlyContinue
}

Write-Host "installed cydonia to $(Join-Path $env:LOCALAPPDATA 'Programs\cydonia\cydonia.exe')"
