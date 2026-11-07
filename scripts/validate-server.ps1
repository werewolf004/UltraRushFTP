<#
.SYNOPSIS
    Checks UltraRushFTP against a real server and writes an anonymised report.

.DESCRIPTION
    Runs `urftp` (the command-line client, same site manager as the interface) through a
    fixed scenario on a site of the site manager: listing, folder upload and download with
    several transfers, download and upload resume, overwrite, optional FXP to a second
    site, then removal of the test files (FTP and FTPS only). Every file is compared by
    SHA-256.

    Credentials are never passed on the command line: the sites come from the site
    manager. The report (report.md and log.txt in the output folder) hides the host, the
    user name, the site names, every IP address and the Windows user folder. Read it before
    sending it anyway: server banners can name the site.

.PARAMETER Site
    Site manager site to check (name, case-insensitive, without its group).
.PARAMETER Folder
    Remote folder where a test folder `urftp-validation-<date>` is created (write access
    needed).
.PARAMETER FxpTo
    Optional second site: the test folder is copied there by FXP, then read back.
.PARAMETER FxpFolder
    Remote folder on the FXP site (default: the same as -Folder).
.PARAMETER Data
    Settings folder (like -f of the interface). Default: the usual one.
.PARAMETER SizeMiB
    Size of the large test file (default 8).
.PARAMETER Insecure
    Accept any TLS certificate (self-signed lab servers).
.PARAMETER Fingerprint
    Expected SHA-256 fingerprint of the FTPS certificate or of the SFTP host key, when the
    site does not store it yet.
.PARAMETER Keep
    Leave the test files on the server.
.PARAMETER Label
    Name given to the server in the report (default "server").
.PARAMETER SelfTest
    Runs the scenario against two local development servers (no real server involved).

.EXAMPLE
    pwsh scripts/validate-server.ps1 -Site "My vsftpd" -Folder /upload -Label vsftpd
.EXAMPLE
    pwsh scripts/validate-server.ps1 -Site "Lab glftpd" -Folder /incoming -FxpTo "Lab ProFTPD" -FxpFolder /pub -Insecure
#>
[CmdletBinding()]
param(
    [string]$Site,
    [string]$Folder,
    [string]$FxpTo,
    [string]$FxpFolder,
    [string]$Data,
    [string]$Urftp,
    [string]$Out,
    [ValidateRange(1, 1024)][int]$SizeMiB = 8,
    [switch]$Insecure,
    [string]$Fingerprint,
    [switch]$Keep,
    [string]$Label = 'server',
    [switch]$SelfTest
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

# ------------------------------------------------------------------ self-test
if ($SelfTest) {
    Push-Location $root
    try {
        cargo build --release --locked -p urftp-cli
        if ($LASTEXITCODE) { throw 'build of urftp failed' }
        cargo build --locked -p urftp-proto --example dev_server
        if ($LASTEXITCODE) { throw 'build of the development server failed' }
    } finally { Pop-Location }
    $work = Join-Path $root 'target/validation/selftest'
    Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force "$work/data" | Out-Null
    $servers = @()
    $ports = @()
    try {
        foreach ($n in 1, 2) {
            $log = Join-Path $work "server$n.txt"
            $servers += Start-Process -FilePath (Join-Path $root 'target/debug/examples/dev_server.exe') -RedirectStandardOutput $log -PassThru -WindowStyle Hidden
            $port = $null
            for ($i = 0; $i -lt 100 -and -not $port; $i++) {
                Start-Sleep -Milliseconds 100
                if (Test-Path $log) { $port = (Select-String -Path $log -Pattern '^PORT=(\d+)' | Select-Object -First 1).Matches.Groups[1].Value }
            }
            if (-not $port) { throw 'development server did not start' }
            $ports += [int]$port
        }
        $lab = { param($id, $name, $port) @{ id = $id; name = $name; host = '127.0.0.1'; port = $port; user = 'test'; password = 'b64:c2VjcmV0'; max_transfers = 3 } }
        @{ version = 1; root = @{ name = ''; groups = @(); sites = @((& $lab 1 'Lab A' $ports[0]), (& $lab 2 'Lab B' $ports[1])) } } |
            ConvertTo-Json -Depth 10 | Set-Content -Encoding utf8NoBOM "$work/data/sites.json"
        & $PSCommandPath -Site 'Lab A' -Folder /incoming -FxpTo 'Lab B' -FxpFolder /incoming -Data "$work/data" -Out "$work/report" -SizeMiB $SizeMiB -Label 'development server'
        exit $LASTEXITCODE
    } finally {
        $servers | ForEach-Object { Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue }
    }
}

if (-not $Site -or -not $Folder) { throw 'give -Site and -Folder (or -SelfTest); see Get-Help scripts/validate-server.ps1' }
if (-not $FxpFolder) { $FxpFolder = $Folder }
if (-not $Urftp) {
    # Next to the script (release archive), else the build of the source tree.
    $Urftp = Join-Path $PSScriptRoot 'urftp.exe'
    if (-not (Test-Path $Urftp)) { $Urftp = Join-Path $root 'target/release/urftp.exe' }
}
if (-not (Test-Path $Urftp)) { throw "urftp.exe not found ($Urftp): build it with 'cargo build --release -p urftp-cli' or give -Urftp" }
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
if (-not $Out) {
    $Out = if (Test-Path (Join-Path $root 'Cargo.toml')) { Join-Path $root "target/validation/$stamp" } else { Join-Path $PSScriptRoot "validation/$stamp" }
}
$work = Join-Path $Out 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

$common = @('-v', '--lang', 'en')
if ($Data) { $common += @('--data', $Data) }
if ($Insecure) { $common += '--insecure' }
if ($Fingerprint) { $common += @('--fingerprint', $Fingerprint) }

# Everything printed by urftp, kept in memory and written once masked.
$log = [System.Collections.Generic.List[string]]::new()
$results = [System.Collections.Generic.List[object]]::new()

function Invoke-Urftp([string[]]$Arguments) {
    $log.Add('')
    $log.Add("### urftp $($Arguments -join ' ')")
    $lines = & $Urftp @common @Arguments 2>&1 | ForEach-Object { "$_" }
    $code = $LASTEXITCODE
    $lines | ForEach-Object { $log.Add($_) }
    $log.Add("### exit code $code")
    [pscustomobject]@{ Code = $code; Lines = @($lines); Text = ($lines -join "`n") }
}

function Add-Result([string]$Step, [string]$Status, [double]$Seconds, [string]$Detail = '') {
    $results.Add([pscustomobject]@{ Step = $Step; Status = $Status; Seconds = [math]::Round($Seconds, 1); Detail = $Detail })
    $mark = @{ ok = 'OK  '; fail = 'FAIL'; warn = 'WARN'; skip = 'SKIP' }[$Status]
    Write-Host ("[{0}] {1}{2}" -f $mark, $Step, $(if ($Detail) { " - $Detail" } else { '' }))
}

function Step([string]$Name, [scriptblock]$Body) {
    $t = [Diagnostics.Stopwatch]::StartNew()
    try {
        $r = & $Body
        $status, $detail = if ($r -is [array]) { $r[0], $r[1] } else { $r, '' }
        Add-Result $Name $status $t.Elapsed.TotalSeconds $detail
    } catch {
        Add-Result $Name 'fail' $t.Elapsed.TotalSeconds $_.Exception.Message
    }
}

# Relative path -> SHA-256 of every file of a tree.
function Get-TreeHashes([string]$Dir) {
    $h = @{}
    Get-ChildItem -LiteralPath $Dir -Recurse -File | ForEach-Object {
        $h[[IO.Path]::GetRelativePath($Dir, $_.FullName).Replace('\', '/')] = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash
    }
    $h
}

function Compare-Trees([string]$Expected, [string]$Actual) {
    if (-not (Test-Path -LiteralPath $Actual)) { return 'fail', 'nothing received' }
    $a = Get-TreeHashes $Expected
    $b = Get-TreeHashes $Actual
    $missing = @($a.Keys | Where-Object { -not $b.ContainsKey($_) })
    $wrong = @($a.Keys | Where-Object { $b.ContainsKey($_) -and $b[$_] -ne $a[$_] })
    $extra = @($b.Keys | Where-Object { -not $a.ContainsKey($_) })
    if ($missing.Count + $wrong.Count + $extra.Count -eq 0) { return 'ok', "$($a.Count) files identical (SHA-256)" }
    'fail', (@(
            $(if ($missing) { "missing: $($missing -join ', ')" }),
            $(if ($wrong) { "different: $($wrong -join ', ')" }),
            $(if ($extra) { "unexpected: $($extra -join ', ')" })) | Where-Object { $_ }) -join '; '
}

function Compare-Files([string]$Expected, [string]$Actual) {
    if (-not (Test-Path -LiteralPath $Actual)) { return 'fail', 'nothing received' }
    if ((Get-FileHash -LiteralPath $Expected).Hash -eq (Get-FileHash -LiteralPath $Actual).Hash) { return 'ok', 'identical (SHA-256)' }
    'fail', "different content ($((Get-Item -LiteralPath $Actual).Length) bytes received, $((Get-Item -LiteralPath $Expected).Length) expected)"
}

function Join-Remote([string]$Dir, [string]$Name) { ($Dir.TrimEnd('/') + '/' + $Name) }

function Test-Ok($r, [string]$Good = 'transfer finished') {
    if ($r.Code -eq 0) { return 'ok', $Good }
    $last = $r.Lines | Where-Object { $_ -match '^(error|erreur)' } | Select-Object -Last 1
    'fail', "exit code $($r.Code)$(if ($last) { ": $last" })"
}

# ------------------------------------------------------------------ the sites
$listing = Invoke-Urftp @('sites')
$siteRe = '^(?<name>.*?)\s+(?<proto>FTPS \((?:explicit|implicit)\)|SFTP|FTP)\s+(?<host>.+):(?<port>\d+)'
function Find-Site([string]$Name) {
    foreach ($l in $listing.Lines) {
        if ($l -match $siteRe) {
            $n = $Matches.name.Trim()
            if ($n -ieq $Name -or $n.EndsWith("/$Name", [StringComparison]::OrdinalIgnoreCase)) {
                return [pscustomobject]@{ Name = $Name; Protocol = $Matches.proto; Host = $Matches.host.Trim('[', ']') }
            }
        }
    }
    throw "site '$Name' not found in the site manager (urftp sites)"
}
$main = Find-Site $Site
$fxp = if ($FxpTo) { Find-Site $FxpTo }
$isFtp = $main.Protocol -ne 'SFTP'

# ------------------------------------------------------------------ test data
$run = "urftp-validation-$stamp"
$src = Join-Path $work "source/$run"
New-Item -ItemType Directory -Force "$src/sub/deeper folder", "$src/many" | Out-Null
$rng = [Random]::new(20261107)
$bytes = [byte[]]::new($SizeMiB * 1MB)
$rng.NextBytes($bytes)
[IO.File]::WriteAllBytes("$src/big.bin", $bytes)
[IO.File]::WriteAllText("$src/été 2026 #1 (copie).txt", "UltraRushFTP – accents, espaces et signes`r`n")
'a' * 100 | Set-Content -NoNewline "$src/sub/a.txt"
'b' * 5000 | Set-Content -NoNewline "$src/sub/deeper folder/b.txt"
1..20 | ForEach-Object { "small file $_`r`n" * $_ | Set-Content -NoNewline ("$src/many/f{0:d2}.txt" -f $_) }
$remoteRun = Join-Remote $Folder $run
# First 40 % of the large file, for the resume steps.
$head = [byte[]]::new([int]($bytes.Length * 0.4))
[Array]::Copy($bytes, $head, $head.Length)
$big = Join-Remote $remoteRun 'big.bin'

Write-Host "UltraRushFTP validation: $($main.Protocol), test folder $run"

# ------------------------------------------------------------------ scenario
Step 'Connect and list the folder' {
    $r = Invoke-Urftp @('ls', "${Site}:$Folder")
    Test-Ok $r 'listing received'
}

if ($isFtp) {
    Step 'Server information (SYST, FEAT, PWD)' {
        $r = Invoke-Urftp @('raw', $Site, 'SYST', 'FEAT', 'PWD')
        if ($r.Code -eq 0) { 'ok', 'replies in log.txt' } else { 'warn', 'one of the commands was refused (see log.txt)' }
    }
}

Step 'Upload a folder (3 transfers)' {
    Test-Ok (Invoke-Urftp @('put', '-n', '3', $src, "${Site}:$Folder"))
}

Step 'List the uploaded folder' {
    $r = Invoke-Urftp @('ls', "${Site}:$remoteRun")
    if ($r.Code -ne 0) { return Test-Ok $r }
    $names = 'big.bin', 'été 2026 #1 (copie).txt', 'sub/', 'many/'
    $miss = @($names | Where-Object { $r.Text -notmatch [regex]::Escape($_) })
    if ($miss) { 'fail', "not in the listing: $($miss -join ', ')" } else { 'ok', 'names, accents and folders shown' }
}

Step 'Download the folder (3 transfers)' {
    $r = Invoke-Urftp @('get', '-n', '3', "${Site}:$remoteRun", "$work/download")
    if ($r.Code -ne 0) { return Test-Ok $r }
    Compare-Trees $src "$work/download/$run"
}

Step 'Resume a download' {
    $part = New-Item -ItemType Directory -Force "$work/resume-get"
    [IO.File]::WriteAllBytes("$part/big.bin", $head)
    $r = Invoke-Urftp @('get', '--exists', 'resume', "${Site}:$big", $part)
    if ($r.Code -ne 0) { return Test-Ok $r }
    $s, $d = Compare-Files "$src/big.bin" "$part/big.bin"
    if ($s -eq 'ok' -and $isFtp -and $r.Text -notmatch '(?m)^> REST [1-9]') { return 'warn', "$d, but no REST sent: downloaded again from the start" }
    $s, $d
}

Step 'Overwrite on upload' {
    Test-Ok (Invoke-Urftp @('put', '--exists', 'overwrite', "$src/big.bin", "${Site}:$remoteRun")) 'file replaced'
}

Step 'Resume an upload' {
    $part = New-Item -ItemType Directory -Force "$work/resume-put"
    [IO.File]::WriteAllBytes("$part/big.bin", $head)
    $r = Invoke-Urftp @('put', '--exists', 'overwrite', "$part/big.bin", "${Site}:$remoteRun")
    if ($r.Code -ne 0) { return 'fail', "partial file not sent: $((Test-Ok $r)[1])" }
    $r = Invoke-Urftp @('put', '--exists', 'resume', "$src/big.bin", "${Site}:$remoteRun")
    if ($r.Code -ne 0) { return Test-Ok $r }
    $sentAgain = $isFtp -and $r.Text -notmatch '(?m)^> (APPE |REST [1-9])'
    $back = New-Item -ItemType Directory -Force "$work/resume-put-check"
    $c = Invoke-Urftp @('get', "${Site}:$big", $back)
    if ($c.Code -ne 0) { return 'fail', "reading back failed: $((Test-Ok $c)[1])" }
    $s, $d = Compare-Files "$src/big.bin" "$back/big.bin"
    if ($s -eq 'ok' -and $sentAgain) { return 'warn', "$d, but neither APPE nor REST sent: uploaded again from the start" }
    $s, $d
}

if ($fxp) {
    $fxpRun = Join-Remote $FxpFolder $run
    Step 'FXP of the folder to the second site' {
        Test-Ok (Invoke-Urftp @('fxp', "${Site}:$remoteRun", "${FxpTo}:$FxpFolder"))
    }
    Step 'Read back the FXP copy' {
        $r = Invoke-Urftp @('get', '-n', '3', "${FxpTo}:$fxpRun", "$work/fxp")
        if ($r.Code -ne 0) { return Test-Ok $r }
        Compare-Trees $src "$work/fxp/$run"
    }
}

# Removal: files first, then folders from the deepest.
function Remove-Remote([string]$Name, [string]$Base, [string]$Protocol) {
    if ($Keep) { return 'skip', '-Keep given: test files left on the server' }
    if ($Protocol -eq 'SFTP') { return 'skip', "SFTP: remove $run by hand (urftp has no delete command yet)" }
    $files = Get-ChildItem -LiteralPath $src -Recurse -File | ForEach-Object { 'DELE ' + (Join-Remote $Base ([IO.Path]::GetRelativePath($src, $_.FullName).Replace('\', '/'))) }
    $dirs = Get-ChildItem -LiteralPath $src -Recurse -Directory | Sort-Object { $_.FullName.Length } -Descending |
        ForEach-Object { 'RMD ' + (Join-Remote $Base ([IO.Path]::GetRelativePath($src, $_.FullName).Replace('\', '/'))) }
    $r = Invoke-Urftp (@('raw', $Name) + $files + $dirs + "RMD $Base")
    if ($r.Code -eq 0) { 'ok', 'removed' } else { 'warn', "some files could not be removed: remove $run by hand" }
}
Step 'Remove the test files' { Remove-Remote $Site $remoteRun $main.Protocol }
if ($fxp) { Step 'Remove the FXP copy' { Remove-Remote $FxpTo $fxpRun $fxp.Protocol } }

# ------------------------------------------------------------------ report
$secrets = [System.Collections.Generic.List[object]]::new()
$users = $log | Select-String -Pattern '^> USER (.+)$' | ForEach-Object { $_.Matches.Groups[1].Value.Trim() } | Sort-Object -Unique
foreach ($u in $users) { if ($u.Length -ge 2 -and $u -notin 'anonymous', 'ftp') { $secrets.Add(@($u, '<user>')) } }
$secrets.Add(@($main.Host, '<host>'))
$secrets.Add(@($Site, '<site>'))
if ($fxp) { $secrets.Add(@($fxp.Host, '<host-2>')); $secrets.Add(@($FxpTo, '<site-2>')) }
$secrets.Add(@($env:USERPROFILE, '%USERPROFILE%'))
$ordered = $secrets | Where-Object { $_[0] } | Sort-Object { $_[0].Length } -Descending

function Hide([string]$Text) {
    # Whole words only: a user "test" must not change "selftest".
    foreach ($s in $ordered) { $Text = [regex]::Replace($Text, '(?<!\w)' + [regex]::Escape($s[0]) + '(?!\w)', $s[1].Replace('$', '$$'), 'IgnoreCase') }
    $Text = [regex]::Replace($Text, '\(?\b\d{1,3},\d{1,3},\d{1,3},\d{1,3},\d{1,3},\d{1,3}\b\)?', '(h,h,h,h,p,p)')
    $Text = [regex]::Replace($Text, '\b(?:\d{1,3}\.){3}\d{1,3}\b', 'x.x.x.x')
    $Text = [regex]::Replace($Text, '\b(?:[0-9A-Fa-f]{1,4}:){4,7}[0-9A-Fa-f]{1,4}\b|\b[0-9A-Fa-f]{1,4}(?::[0-9A-Fa-f]{1,4})*::(?:[0-9A-Fa-f]{1,4}(?::[0-9A-Fa-f]{1,4})*)?\b', 'x:x::x')
    $Text
}

$version = (Select-String -Path (Join-Path $root 'Cargo.toml') -Pattern '^version\s*=\s*"(.+)"' -ErrorAction SilentlyContinue | Select-Object -First 1).Matches.Groups[1].Value
if (-not $version) { $version = (Get-Item -LiteralPath $Urftp).VersionInfo.ProductVersion }
$fails = @($results | Where-Object Status -EQ 'fail').Count
$warns = @($results | Where-Object Status -EQ 'warn').Count
$md = [System.Collections.Generic.List[string]]::new()
$md.Add("# UltraRushFTP validation — $Label")
$md.Add('')
$md.Add("- Date: $(Get-Date -Format 'yyyy-MM-dd HH:mm')")
$md.Add("- UltraRushFTP: $(if ($version) { $version } else { 'unknown' }); $([Environment]::OSVersion.VersionString); PowerShell $($PSVersionTable.PSVersion)")
$md.Add("- Protocol: $($main.Protocol)$(if ($fxp) { "; FXP to $($fxp.Protocol)" }); large file: $SizeMiB MiB$(if ($Insecure) { '; any certificate accepted' })")
$md.Add("- Result: $(if ($fails) { "**$fails failed**" } else { 'all passed' })$(if ($warns) { ", $warns warning(s)" })")
$md.Add('')
$md.Add('| Step | Result | Time (s) | Detail |')
$md.Add('|---|---|---:|---|')
foreach ($r in $results) { $md.Add("| $($r.Step) | $($r.Status) | $($r.Seconds) | $((Hide $r.Detail).Replace('|', '\|')) |") }
$md.Add('')
$md.Add('The protocol log is in `log.txt`. Host, user, site names, IP addresses and the Windows')
$md.Add('user folder are hidden; read both files before sending them (banners may name the site).')
Set-Content -Encoding utf8NoBOM -LiteralPath (Join-Path $Out 'report.md') -Value ($md -join "`n")
Set-Content -Encoding utf8NoBOM -LiteralPath (Join-Path $Out 'log.txt') -Value (Hide ($log -join "`n"))
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue

Write-Host ''
Write-Host "Report: $(Join-Path $Out 'report.md')"
Write-Host "Log:    $(Join-Path $Out 'log.txt')"
exit $(if ($fails) { 1 } else { 0 })
