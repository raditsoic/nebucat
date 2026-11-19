<#
.SYNOPSIS
  Installs nebucat from the latest GitHub release into %LOCALAPPDATA%\Programs\nebucat.
.PARAMETER WithEverything
  Also installs Everything (voidtools) with winget. Needed only for / file search.
.PARAMETER Repo
  GitHub repository, owner/name.
#>
param(
    [string]$Repo = 'raditsoic/nebucat',
    [switch]$WithEverything
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$dir = Join-Path $env:LOCALAPPDATA 'Programs\nebucat'
$release = Invoke-RestMethod "https://api.github.com/repos/$Repo/releases/latest" -Headers @{ 'User-Agent' = 'nebucat-installer' }
$zipAsset = $release.assets | Where-Object { $_.name -like 'nebucat-*-windows-x64.zip' } | Select-Object -First 1
$sumsAsset = $release.assets | Where-Object { $_.name -eq 'SHA256SUMS' } | Select-Object -First 1
if (-not $zipAsset -or -not $sumsAsset) { throw "Release $($release.tag_name) has no zip or SHA256SUMS asset." }

$tmp = Join-Path ([IO.Path]::GetTempPath()) ("nebucat-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    $zip = Join-Path $tmp $zipAsset.name
    Invoke-WebRequest $zipAsset.browser_download_url -OutFile $zip
    $sums = (Invoke-WebRequest $sumsAsset.browser_download_url -UseBasicParsing).Content
    $line = ($sums -split "`n" | Where-Object { $_ -match [regex]::Escape($zipAsset.name) } | Select-Object -First 1)
    if (-not $line) { throw 'SHA256SUMS has no entry for the zip.' }
    $expected = ($line -split '\s+')[0].ToLower()
    $actual = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
    if ($expected -ne $actual) { throw "Checksum mismatch: expected $expected, got $actual." }

    Get-Process nebucat -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep -Milliseconds 300
    if (Test-Path $dir) { Remove-Item $dir -Recurse -Force }
    Expand-Archive $zip -DestinationPath $dir
}
finally {
    Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
}

$shortcut = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\nebucat.lnk'
$shell = New-Object -ComObject WScript.Shell
$lnk = $shell.CreateShortcut($shortcut)
$lnk.TargetPath = Join-Path $dir 'nebucat.exe'
$lnk.WorkingDirectory = $dir
$lnk.Save()

if ($WithEverything) {
    winget install -e --id voidtools.Everything --accept-package-agreements --accept-source-agreements
}

Start-Process (Join-Path $dir 'nebucat.exe')
Write-Host "Installed nebucat $($release.tag_name) to $dir. Press Alt+Space."
if (-not $WithEverything) {
    Write-Host 'File search (/) needs Everything: winget install voidtools.Everything'
}
