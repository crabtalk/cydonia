# Installs the latest cydonia release on Windows (x86_64).
#
#   irm https://cydonia.sh/install.ps1 | iex
#
# The zip is unpacked into %LOCALAPPDATA%\Programs\cydonia, with a Start menu
# shortcut to it.
$ErrorActionPreference = 'Stop'

$url = 'https://github.com/crabtalk/cydonia/releases/latest/download/cydonia-windows-x86_64.zip'
$dir = Join-Path $env:LOCALAPPDATA 'Programs\cydonia'
$zip = Join-Path ([IO.Path]::GetTempPath()) "cydonia-$([guid]::NewGuid()).zip"

Write-Host "downloading $url"
Invoke-WebRequest -Uri $url -OutFile $zip -UseBasicParsing
try {
    New-Item -ItemType Directory -Force $dir | Out-Null
    Expand-Archive -Path $zip -DestinationPath $dir -Force
} finally {
    Remove-Item $zip -ErrorAction SilentlyContinue
}

$exe = Join-Path $dir 'cydonia.exe'
$shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) 'Cydonia.lnk'
$link = (New-Object -ComObject WScript.Shell).CreateShortcut($shortcut)
$link.TargetPath = $exe
$link.WorkingDirectory = $dir
$link.Save()

Write-Host "installed cydonia to $exe"
