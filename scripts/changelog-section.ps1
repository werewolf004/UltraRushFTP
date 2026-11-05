# Prints the CHANGELOG.md section of one version (Keep a Changelog format), used as the
# GitHub release notes. Fails when the section is missing or empty, so that no release is
# published without its changelog.
#
#   pwsh scripts/changelog-section.ps1 -Version 0.1.0 [-Changelog CHANGELOG.md]
param(
    [Parameter(Mandatory)] [string] $Version,
    [string] $Changelog = 'CHANGELOG.md'
)
$ErrorActionPreference = 'Stop'
$lines = Get-Content -LiteralPath $Changelog -Encoding utf8
$header = '^## \[' + [regex]::Escape($Version) + '\]'
$start = -1
for ($i = 0; $i -lt $lines.Count; $i++) {
    if ($lines[$i] -match $header) { $start = $i + 1; break }
}
if ($start -lt 0) { throw "No '## [$Version]' section in $Changelog" }
$end = $lines.Count
for ($i = $start; $i -lt $lines.Count; $i++) {
    if ($lines[$i] -match '^## \[') { $end = $i; break }
}
$body = ($lines[$start..($end - 1)] -join "`n").Trim()
if (-not $body) { throw "Section [$Version] of $Changelog is empty" }
$body
