<#
.SYNOPSIS
  Removes nebucat: the program folder, Start Menu shortcut and autostart entry.
.PARAMETER Purge
  Also deletes %APPDATA%\nebucat (config, themes, plugins, history).
#>
param([switch]$Purge)

$ErrorActionPreference = 'Stop'
Get-Process nebucat -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 300

Remove-Item (Join-Path $env:LOCALAPPDATA 'Programs\nebucat') -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item (Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\nebucat.lnk') -Force -ErrorAction SilentlyContinue
Remove-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name nebucat -ErrorAction SilentlyContinue
Remove-Item (Join-Path $env:LOCALAPPDATA 'nebucat') -Recurse -Force -ErrorAction SilentlyContinue

if ($Purge) {
    Remove-Item (Join-Path $env:APPDATA 'nebucat') -Recurse -Force -ErrorAction SilentlyContinue
    Write-Host 'Removed nebucat and its settings.'
} else {
    Write-Host 'Removed nebucat. Settings kept in %APPDATA%\nebucat (run with -Purge to delete them).'
}
