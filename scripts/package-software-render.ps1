# Software rendering pack: Mesa llvmpipe (OpenGL drawn by the processor) for Windows
# without Direct3D 12, such as Windows Server 2016 in a virtual machine without 3D
# acceleration. Its opengl32.dll, copied next to UltraRushFTP.exe, is used when no graphics
# card and no Direct3D 12 software renderer (WARP) can draw the window.
#
# The build comes from https://github.com/mmozeiko/build-mesa (Mesa statically linked into a
# single DLL); the archive is checked against the SHA-256 below before anything is used.
#
#   pwsh scripts/package-software-render.ps1 -Version 0.15.1
# Prints the paths of the files to publish, one per line, on the success stream.
param(
    [Parameter(Mandatory)] [string] $Version
)
$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')
Set-Location $root

$mesaVersion = '26.3.0'
$archive = "mesa-llvmpipe-x64-$mesaVersion.7z"
$url = "https://github.com/mmozeiko/build-mesa/releases/download/$mesaVersion/$archive"
$sha256 = '11dc079e356f33ce2ff84aa399c975da1a7bdd4c470f6eef4cf359ad8b9ca80f'

$name = "UltraRushFTP-$Version-windows-x64-software-render"
$dist = Join-Path $root 'dist'
$work = Join-Path $dist 'mesa-download'
$dir = Join-Path $dist $name
Remove-Item -Recurse -Force $work, $dir, "$dist/$name.zip" -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work, $dir | Out-Null

Invoke-WebRequest -Uri $url -OutFile "$work/$archive"
$got = (Get-FileHash "$work/$archive" -Algorithm SHA256).Hash.ToLower()
if ($got -ne $sha256) { throw "${archive}: SHA-256 $got, expected $sha256" }
# The tar of Windows (libarchive) reads 7z archives.
tar -xf "$work/$archive" -C $work opengl32.dll
if ($LASTEXITCODE -ne 0) { throw "tar failed ($LASTEXITCODE)" }

Copy-Item "$work/opengl32.dll" $dir
Copy-Item packaging/software-render/README.txt, packaging/software-render/LICENSES.txt $dir
Remove-Item -Recurse -Force $work

$zip = "$dist/$name.zip"
Compress-Archive -Path "$dir/*" -DestinationPath $zip
$hash = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
[IO.File]::WriteAllText("$zip.sha256", "$hash  $(Split-Path -Leaf $zip)`n")
$zip, "$zip.sha256"
