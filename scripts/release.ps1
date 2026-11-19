<#
.SYNOPSIS
  Builds the release zip and SHA256SUMS in dist\. Run from anywhere on Windows.
.PARAMETER Publish
  Also create a GitHub release with the GitHub CLI (gh), tagging v<version>.
#>
param([switch]$Publish)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

# Pinned so a changed upstream zip fails the build instead of shipping unreviewed.
$sdkUrl = 'https://www.voidtools.com/Everything-SDK.zip'
$sdkSha256 = 'f5716d9513cce6b462b5170a0a2e7e081e191d9bcac6774f5decc061497df443'

$version = (Select-String -Path 'Cargo.toml' -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
$name = "nebucat-v$version-windows-x64"
$dist = Join-Path $root 'dist'
$stage = Join-Path $dist $name

cargo test -p nebucat-core --locked
if ($LASTEXITCODE) { throw 'core tests failed' }
cargo build --release -p nebucat --locked
if ($LASTEXITCODE) { throw 'release build failed' }

if (Test-Path $dist) { Remove-Item $dist -Recurse -Force }
New-Item -ItemType Directory -Path $stage | Out-Null

$sdkZip = Join-Path $dist 'sdk.zip'
Invoke-WebRequest $sdkUrl -OutFile $sdkZip
$actual = (Get-FileHash $sdkZip -Algorithm SHA256).Hash.ToLower()
if ($actual -ne $sdkSha256) {
    throw "Everything SDK zip hash changed ($actual). Review the new SDK license and DLL, then update `$sdkSha256."
}
Expand-Archive $sdkZip -DestinationPath (Join-Path $dist 'sdk')
Copy-Item (Join-Path $dist 'sdk\dll\Everything64.dll') $stage

Copy-Item 'target\release\nebucat.exe' $stage
Copy-Item 'LICENSE', 'THIRD_PARTY.md', 'README.md', 'CHANGELOG.md' $stage
Copy-Item 'assets\fonts\OFL.txt', 'assets\fonts\OFL-Mono.txt', 'assets\fonts\OFL-DepartureMono.txt' $stage
Copy-Item 'examples\plugins\hello' (Join-Path $stage 'examples\plugins\hello') -Recurse

$zip = Join-Path $dist "$name.zip"
# Compress-Archive in Windows PowerShell 5.1 writes backslash paths, which other unzip tools mangle.
tar.exe -a -c -f $zip -C $stage .
if ($LASTEXITCODE) { throw 'zip creation failed' }
$hash = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
"$hash  $name.zip" | Set-Content (Join-Path $dist 'SHA256SUMS') -Encoding ascii
Remove-Item $stage, (Join-Path $dist 'sdk'), $sdkZip -Recurse -Force

Write-Host "Built $zip"
Write-Host "SHA256 $hash"

if ($Publish) {
    gh release create "v$version" $zip (Join-Path $dist 'SHA256SUMS') --title "nebucat $version" --notes-file CHANGELOG.md
    if ($LASTEXITCODE) { throw 'gh release create failed' }
}
