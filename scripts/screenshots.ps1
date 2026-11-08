# Regenerates the documentation screenshots (docs/images/*.png) in English and French.
#
# Everything shown is demo data: the local development server (127.0.0.1, test/secret),
# fictitious example.com / example.net / example.org sites and a demo folder under
# target/demo. Nothing from the user's real site book, settings or home folder is used.
# The application is drawn off screen (wgpu, through an ignored test of urftp-gui): no
# window opens, nothing comes to the foreground, and the mIRC link window is not created,
# so a running FTPRush or UltraRushFTP is never contacted.
#
#   pwsh scripts/screenshots.ps1 [-Langs en,fr]
param([string[]] $Langs = @('en', 'fr'))
$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')
Set-Location $root

cargo build --locked -p urftp-proto --example dev_server
# The off-screen renderer: test binary of urftp-gui (release, for the transfer speed).
$built = cargo test --release --locked -p urftp-gui --no-run --message-format=json | ForEach-Object { $_ | ConvertFrom-Json } |
    Where-Object { $_.reason -eq 'compiler-artifact' -and $_.target.name -eq 'UltraRushFTP' -and $_.profile.test -and $_.executable }
$renderer = @($built)[-1].executable
if (-not $renderer) { throw 'test binary of urftp-gui not found' }

# ------------------------------------------------------------------ demo server
$serverLog = Join-Path $env:TEMP "urftp-demo-server-$PID.txt"
$server = Start-Process -FilePath target/debug/examples/dev_server.exe -RedirectStandardOutput $serverLog -PassThru -WindowStyle Hidden
$port = $null
for ($i = 0; $i -lt 50 -and -not $port; $i++) {
    Start-Sleep -Milliseconds 100
    if (Test-Path $serverLog) { $port = (Select-String -Path $serverLog -Pattern '^PORT=(\d+)' | Select-Object -First 1).Matches.Groups[1].Value }
}
if (-not $port) { Stop-Process -Id $server.Id -Force; throw 'demo server did not start' }

# ------------------------------------------------------------------ demo data
$demo = Join-Path $root 'target/demo'
$data = Join-Path $demo 'data'
$local = Join-Path $demo 'local'
Remove-Item -Recurse -Force $demo -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $data, "$local/Downloads", "$local/Uploads" | Out-Null
1..3 | ForEach-Object { "demo report $_" | Set-Content "$local/Uploads/report-$_.txt" }
$sites = @{
    version = 1
    root = @{
        name = ''; sites = @()
        groups = @(
            @{ name = 'Demo'; groups = @(); sites = @(
                    @{ id = 1; name = 'Local demo'; host = '127.0.0.1'; port = [int]$port; user = 'test'; password = 'b64:c2VjcmV0'
                        max_transfers = 3; remote_dir = '/pub'; local_dir = $local; alt_hosts = @("localhost:$port")
                        settings = @{ retries = 5; retry_delay_secs = 10; connect_timeout_secs = 20; io_timeout_secs = 60; client_name = 'UltraRushFTP'
                            upload_account = @{ user = 'upload'; password = 'b64:dXBsb2Fk' } }
                        bookmarks = @(@{ name = 'releases'; remote_dir = '/pub/Releases'; local_dir = '' })
                        notes = 'Development server: cargo run -p urftp-proto --example dev_server' },
                    @{ id = 2; name = 'Example FTPS'; host = 'ftp.example.com'; port = 21; protocol = 'ftps_explicit'; user = 'demo'; max_transfers = 4
                        description = 'Releases'; proxy = @{ named = 'Office' }; http = @{ base_url = 'https://www.example.com/files'; ftp_root = '/pub' } },
                    @{ id = 3; name = 'Example SFTP'; host = 'sftp.example.org'; port = 22; protocol = 'sftp'; user = 'demo' }) },
            @{ name = 'Mirrors'; groups = @(); sites = @(
                    @{ id = 4; name = 'Mirror A'; host = 'mirror-a.example.net'; port = 990; protocol = 'ftps_implicit'; anonymous = $true },
                    @{ id = 5; name = 'Mirror B'; host = 'mirror-b.example.net'; port = 21; user = 'demo'; transfer_mode = 'active' }) })
    }
}
$sites | ConvertTo-Json -Depth 10 | Set-Content -Encoding utf8NoBOM "$data/sites.json"
$rule = { param($mask, $regex, $target, $up, $down, $fxp)
    @{ mask = @{ mask = $mask; regex = $regex }; action = 'Skip'; target = $target; directions = @{ upload = $up; download = $down; fxp = $fxp }; full_path = $false } }
@{
    version = 1; dark_theme = $true
    transfer = @{ download_limit_kib = 1200 }
    skiplist = @{ enabled = $true; skip_zero_byte_files = $true; skip_empty_dirs = $false
        rules = @((& $rule '*.jpg' $false 'Files' $true $true $true), (& $rule 'Sample' $false 'Dirs' $true $true $true), (& $rule '^.*\.(tmp|part)$' $true 'Files' $true $false $false)) }
    priorities = @(@{ mask = '*.sfv'; regex = $false }, @{ mask = '*.nfo'; regex = $false })
    proxies = @(@{ name = 'Office'; proxy = @{ kind = 'http_tunnel'; host = 'proxy.example.net'; port = 3128; data_channel = $true } })
} | ConvertTo-Json -Depth 10 | Set-Content -Encoding utf8NoBOM "$data/settings.json"
$script = Join-Path $demo 'transfer.pas'
"begin`n  RushApp.FTP.Transfer(0, 'Local demo', '/pub/Releases', 'UltraRush.Demo-2026', '', '$local\Downloads', '', RS_DOWN or RS_DIRSRC or RS_DIRDES, '', '', '', '', '', 0, 0, 0, 0, 0, 0, 0);`nend." |
    Set-Content -Encoding utf8NoBOM $script

# ------------------------------------------------------------------ capture
function Shot([string] $out, [string[]] $arguments, [hashtable] $envs, [double] $wait, [switch] $KeepQueue) {
    # The queue left by the previous shot would be offered at startup.
    if (-not $KeepQueue) { Remove-Item -Force "$data/queue.json" -ErrorAction SilentlyContinue }
    $all = $envs + @{ URUSHFTP_SHOT_OUT = $out; URUSHFTP_SHOT_ARGS = ($arguments -join "`n"); URUSHFTP_SHOT_WAIT = "$wait"; URUSHFTP_SHOT_HOME = $local }
    foreach ($k in $all.Keys) { Set-Item "env:$k" $all[$k] }
    # An image left by a previous run must not pass for this one.
    Remove-Item -Force $out -ErrorAction SilentlyContinue
    try {
        $log = & $renderer --ignored --exact app::shots::render_screenshot --test-threads 1 2>&1
        if ($LASTEXITCODE -or -not (Test-Path $out)) { $log | ForEach-Object { Write-Host "    $_" }; throw "capture failed (exit $LASTEXITCODE, renderer $renderer): $out" }
        Write-Host "  $out"
    } finally {
        foreach ($k in $all.Keys) { Remove-Item "env:$k" -ErrorAction SilentlyContinue }
    }
}

$images = Join-Path $root 'docs/images'
New-Item -ItemType Directory -Force $images | Out-Null
$base = @("-f`"$data`"", '-c"Local demo"')
try {
    foreach ($lang in $Langs) {
        Write-Host "[$lang]"
        $common = @{ URUSHFTP_LANG = $lang }
        Remove-Item -Recurse -Force "$local/Downloads/*" -ErrorAction SilentlyContinue
        Shot "$images/main.$lang.png" ($base + "-s`"$script`"") $common 3
        Remove-Item -Recurse -Force "$local/Downloads/*" -ErrorAction SilentlyContinue
        Shot "$images/main-light.$lang.png" ($base + "-s`"$script`"") ($common + @{ URUSHFTP_THEME = 'light' }) 3
        # Queues of two tabs left unfinished: they are offered one by one.
        $item = { param($ss, $sp, $ds, $dp, $size) @{ src_site = $ss; src_path = $sp; dst_site = $ds; dst_path = $dp; is_dir = $false; size = $size; priority = $null; state = 'waiting' } }
        @{ version = 1; tabs = @(
                @{ title = '127.0.0.1'; items = @((& $item 'Local demo' '/pub/Releases/UltraRush.Demo-2026/demo.r00' $null "$local\Downloads\demo.r00" 1048576),
                        (& $item 'Local demo' '/pub/Releases/UltraRush.Demo-2026/demo.r01' $null "$local\Downloads\demo.r01" 1048576)) },
                @{ title = 'ftp.example.com'; items = @((& $item $null "$local\Uploads\report-1.txt" 'Example FTPS' '/incoming/report-1.txt' 14)) }) } |
            ConvertTo-Json -Depth 10 | Set-Content -Encoding utf8NoBOM "$data/queue.json"
        Shot "$images/queue-restore.$lang.png" $base $common 3 -KeepQueue
        Shot "$images/site-manager.$lang.png" $base ($common + @{ URUSHFTP_OPEN_SITES = '1' }) 3
        Shot "$images/site-settings.$lang.png" $base ($common + @{ URUSHFTP_OPEN_SITES = '1'; URUSHFTP_SITE_SETTINGS = '1' }) 3
        Shot "$images/shortcuts.$lang.png" $base ($common + @{ URUSHFTP_OPEN_SHORTCUTS = '1' }) 3
        $pages = @{ 0 = 'general'; 1 = 'transfers'; 3 = 'existing-file'; 4 = 'skiplist'; 6 = 'proxy'; 7 = 'ident'; 8 = 'keep-alive'; 9 = 'connection'; 10 = 'new-site'; 11 = 'advanced'; 12 = 'browse'; 13 = 'interface'; 14 = 'shortcuts'; 15 = 'notifications'; 16 = 'security' }
        foreach ($page in $pages.Keys | Sort-Object) {
            Shot "$images/options-$($pages[$page]).$lang.png" $base ($common + @{ URUSHFTP_OPEN_OPTIONS = "$page" }) 3
        }
    }
} finally {
    Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue
}
