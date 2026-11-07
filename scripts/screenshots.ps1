# Regenerates the documentation screenshots (docs/images/*.png) in English and French.
#
# Everything shown is demo data: the local development server (127.0.0.1, test/secret),
# fictitious example.com / example.net / example.org sites and a demo folder under
# target/demo. Nothing from the user's real site book, settings or home folder is used.
# The application runs with a private IPC window class, so a running FTPRush or
# UltraRushFTP is never contacted.
#
#   pwsh scripts/screenshots.ps1 [-Langs en,fr]
param([string[]] $Langs = @('en', 'fr'))
$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')
Set-Location $root

cargo build --release --locked -p urftp-gui
cargo build --locked -p urftp-proto --example dev_server

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class UrftpShot {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
}
"@
[UrftpShot]::SetProcessDPIAware() | Out-Null

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
                    @{ id = 2; name = 'Example FTPS'; host = 'ftp.example.com'; port = 21; protocol = 'ftps_explicit'; user = 'demo'; max_transfers = 4 },
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
} | ConvertTo-Json -Depth 10 | Set-Content -Encoding utf8NoBOM "$data/settings.json"
$script = Join-Path $demo 'transfer.pas'
"begin`n  RushApp.FTP.Transfer(0, 'Local demo', '/pub/Releases', 'UltraRush.Demo-2026', '', '$local\Downloads', '', RS_DOWN or RS_DIRSRC or RS_DIRDES, '', '', '', '', '', 0, 0, 0, 0, 0, 0, 0);`nend." |
    Set-Content -Encoding utf8NoBOM $script

# ------------------------------------------------------------------ capture
function Shot([string] $out, [string] $arguments, [hashtable] $envs, [double] $wait) {
    foreach ($k in $envs.Keys) { Set-Item "env:$k" $envs[$k] }
    $p = Start-Process -FilePath target/release/UltraRushFTP.exe -ArgumentList $arguments -PassThru
    try {
        Start-Sleep -Milliseconds ([int]($wait * 1000))
        $p.Refresh()
        $h = $p.MainWindowHandle
        if ($h -eq [IntPtr]::Zero) { throw "no window for $out" }
        [UrftpShot]::SetForegroundWindow($h) | Out-Null
        Start-Sleep -Milliseconds 600
        $r = New-Object UrftpShot+RECT
        [UrftpShot]::GetWindowRect($h, [ref]$r) | Out-Null
        $bmp = New-Object System.Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        $hdc = $g.GetHdc()
        [UrftpShot]::PrintWindow($h, $hdc, 2) | Out-Null
        $g.ReleaseHdc($hdc)
        $bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
        $g.Dispose(); $bmp.Dispose()
        Write-Host "  $out"
    } finally {
        Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
        foreach ($k in $envs.Keys) { Remove-Item "env:$k" -ErrorAction SilentlyContinue }
    }
}

$images = Join-Path $root 'docs/images'
New-Item -ItemType Directory -Force $images | Out-Null
$env:URUSHFTP_IPC_CLASS = "TfmRush.UltraRushFTP.Screenshots.$PID"
$base = "-f`"$data`" -c`"Local demo`""
try {
    foreach ($lang in $Langs) {
        Write-Host "[$lang]"
        $common = @{ URUSHFTP_LANG = $lang }
        Remove-Item -Recurse -Force "$local/Downloads/*" -ErrorAction SilentlyContinue
        Shot "$images/main.$lang.png" "$base -s`"$script`"" $common 3
        Remove-Item -Recurse -Force "$local/Downloads/*" -ErrorAction SilentlyContinue
        Shot "$images/main-light.$lang.png" "$base -s`"$script`"" ($common + @{ URUSHFTP_THEME = 'light' }) 3
        Shot "$images/site-manager.$lang.png" $base ($common + @{ URUSHFTP_OPEN_SITES = '1' }) 3
        Shot "$images/site-settings.$lang.png" $base ($common + @{ URUSHFTP_OPEN_SITES = '1'; URUSHFTP_SITE_SETTINGS = '1' }) 3
        Shot "$images/shortcuts.$lang.png" $base ($common + @{ URUSHFTP_OPEN_SHORTCUTS = '1' }) 3
        $pages = @{ 0 = 'general'; 1 = 'transfers'; 3 = 'existing-file'; 4 = 'skiplist'; 6 = 'proxy'; 7 = 'ident'; 8 = 'keep-alive' }
        foreach ($page in $pages.Keys | Sort-Object) {
            Shot "$images/options-$($pages[$page]).$lang.png" $base ($common + @{ URUSHFTP_OPEN_OPTIONS = "$page" }) 3
        }
    }
} finally {
    Remove-Item env:URUSHFTP_IPC_CLASS -ErrorAction SilentlyContinue
    Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue
}
