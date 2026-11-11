# Packages a release build: signs the binaries (when a certificate is configured, see
# scripts/sign.ps1), gathers them with the documentation, then writes the portable zip and
# the installer (Inno Setup), each with its .sha256 file, in dist/.
#
# Expects the release build already done:
#   cargo build --release --locked -p urftp-gui -p urftp-cli -p rushmirc
#   cargo build --release --locked -p rushmirc --target i686-pc-windows-msvc
#   pwsh scripts/generate-dependencies.ps1 -Notices
#
#   pwsh scripts/package.ps1 -Version 0.9.0 [-NoInstaller]
# Prints the paths of the files to publish, one per line, on the success stream.
param(
    [Parameter(Mandatory)] [string] $Version,
    [switch] $NoInstaller
)
$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')
Set-Location $root

$name = "UltraRushFTP-$Version-windows-x64"
$dist = Join-Path $root 'dist'
$dir = Join-Path $dist $name
Remove-Item -Recurse -Force $dir, "$dist/$name.zip", "$dist/$name-setup.exe" -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force "$dir/mirc/x64", "$dir/mirc/x86" | Out-Null

Copy-Item target/release/UltraRushFTP.exe, target/release/urftp.exe $dir
Copy-Item scripts/validate-server.ps1 $dir
Copy-Item target/release/rushmirc.dll "$dir/mirc/x64/"
Copy-Item target/i686-pc-windows-msvc/release/rushmirc.dll "$dir/mirc/x86/"
Get-ChildItem -File -Include 'LICENSE*', 'README*.md', 'CHANGELOG*.md', 'SECURITY*.md', 'ROADMAP*.md', 'CONTRIBUTING*.md', 'THIRD-PARTY-LICENSES.txt' -Path * |
    Copy-Item -Destination $dir
Copy-Item -Recurse docs "$dir/docs"

& "$PSScriptRoot/sign.ps1" -Files "$dir/UltraRushFTP.exe", "$dir/urftp.exe", "$dir/mirc/x64/rushmirc.dll", "$dir/mirc/x86/rushmirc.dll" | Write-Host

function Write-Sha256([string] $file) {
    $hash = (Get-FileHash $file -Algorithm SHA256).Hash.ToLower()
    # LF line ending, so that `sha256sum -c` accepts it too.
    [IO.File]::WriteAllText("$file.sha256", "$hash  $(Split-Path -Leaf $file)`n")
}

$outputs = @()
Compress-Archive -Path "$dir/*" -DestinationPath "$dist/$name.zip"
Write-Sha256 "$dist/$name.zip"
$outputs += "$dist/$name.zip", "$dist/$name.zip.sha256"

if (-not $NoInstaller) {
    $iscc = @(
        (Get-Command ISCC.exe -ErrorAction SilentlyContinue).Source
        "${env:ProgramFiles(x86)}/Inno Setup 6/ISCC.exe"
        "$env:ProgramFiles/Inno Setup 6/ISCC.exe"
        "$env:LOCALAPPDATA/Programs/Inno Setup 6/ISCC.exe"
    ) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
    if (-not $iscc) { throw 'Inno Setup 6 (ISCC.exe) not found: install it, or pass -NoInstaller' }
    # Inno Setup wants a numeric version for the file properties: 0.9.0-rc1 -> 0.9.0.
    $numeric = ($Version -split '[-+]')[0]
    & $iscc /Q "/DAppVersion=$Version" "/DNumericVersion=$numeric" "/DSourceDir=$dir" "/DOutputDir=$dist" installer/UltraRushFTP.iss | Write-Host
    if ($LASTEXITCODE -ne 0) { throw "ISCC failed ($LASTEXITCODE)" }
    $setup = "$dist/$name-setup.exe"
    & "$PSScriptRoot/sign.ps1" -Files $setup | Write-Host
    Write-Sha256 $setup
    $outputs += $setup, "$setup.sha256"
}

$outputs
