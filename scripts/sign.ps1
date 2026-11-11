# Signs files with Authenticode (signtool of the Windows SDK, SHA-256, RFC 3161 timestamp).
#
# The certificate comes from the environment, never from the repository:
#   SIGN_PFX_BASE64    the .pfx file, base64-encoded (GitHub secret)
#   SIGN_PFX_PASSWORD  its password (GitHub secret)
#   SIGN_TIMESTAMP_URL timestamp server (default: http://timestamp.digicert.com)
# Without SIGN_PFX_BASE64 nothing is signed and the script says so: releases are then
# published unsigned, as before.
#
#   pwsh scripts/sign.ps1 -Files a.exe, b.dll
param([Parameter(Mandatory)] [string[]] $Files)
$ErrorActionPreference = 'Stop'

if (-not $env:SIGN_PFX_BASE64) {
    Write-Host 'No signing certificate (SIGN_PFX_BASE64 is empty): files left unsigned.'
    return
}

$kits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10/bin'
$signtool = Get-ChildItem -Path $kits -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match '\\x64\\' } | Sort-Object FullName | Select-Object -Last 1
if (-not $signtool) { throw 'signtool.exe not found (Windows SDK)' }

$pfx = Join-Path ([IO.Path]::GetTempPath()) "urftp-sign-$PID.pfx"
try {
    [IO.File]::WriteAllBytes($pfx, [Convert]::FromBase64String($env:SIGN_PFX_BASE64))
    $timestamp = if ($env:SIGN_TIMESTAMP_URL) { $env:SIGN_TIMESTAMP_URL } else { 'http://timestamp.digicert.com' }
    $signArgs = @('sign', '/fd', 'SHA256', '/f', $pfx, '/tr', $timestamp, '/td', 'SHA256', '/d', 'UltraRushFTP')
    if ($env:SIGN_PFX_PASSWORD) { $signArgs += @('/p', $env:SIGN_PFX_PASSWORD) }
    & $signtool.FullName @signArgs @Files
    if ($LASTEXITCODE -ne 0) { throw "signtool failed ($LASTEXITCODE)" }
    & $signtool.FullName verify /pa @Files
    if ($LASTEXITCODE -ne 0) { throw "signature check failed ($LASTEXITCODE)" }
} finally {
    Remove-Item -Force $pfx -ErrorAction SilentlyContinue
}
