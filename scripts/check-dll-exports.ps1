# Checks that a rushmirc.dll exports exactly the functions mIRC scripts call, and that
# its architecture is the expected one. Reads the PE export table without loading the DLL.
#
#   pwsh scripts/check-dll-exports.ps1 -Path target/release/rushmirc.dll -Bits 64
param(
    [Parameter(Mandatory)] [string] $Path,
    [Parameter(Mandatory)] [ValidateSet(32, 64)] [int] $Bits
)
$ErrorActionPreference = 'Stop'
$expected = 'DisableCallback', 'EnableCallback', 'Invoke', 'LoadDll', 'RushCommand', 'RushScript', 'UnloadDll'

$b = [IO.File]::ReadAllBytes((Resolve-Path $Path))
function U16($o) { [BitConverter]::ToUInt16($b, $o) }
function U32($o) { [BitConverter]::ToUInt32($b, $o) }

$pe = U32 0x3C
if ((U32 $pe) -ne 0x4550) { throw "$Path is not a PE file" }
$sections = U16 ($pe + 6)
$optSize = U16 ($pe + 20)
$opt = $pe + 24
$is64 = (U16 $opt) -eq 0x20B
$actualBits = if ($is64) { 64 } else { 32 }
if ($actualBits -ne $Bits) { throw "$Path is $actualBits-bit, expected $Bits-bit" }
$exportDir = if ($is64) { $opt + 112 } else { $opt + 96 }
$exportRva = U32 $exportDir
if ($exportRva -eq 0) { throw "$Path has no export table" }
$sectionTable = $opt + $optSize

function Offset($rva) {
    for ($i = 0; $i -lt $sections; $i++) {
        $s = $sectionTable + 40 * $i
        $va = U32 ($s + 12)
        $size = [Math]::Max((U32 ($s + 8)), (U32 ($s + 16)))
        if ($rva -ge $va -and $rva -lt $va + $size) { return (U32 ($s + 20)) + ($rva - $va) }
    }
    throw "RVA 0x{0:X} outside every section" -f $rva
}

$e = Offset $exportRva
$count = U32 ($e + 24)
$names = Offset (U32 ($e + 32))
$actual = for ($i = 0; $i -lt $count; $i++) {
    $o = Offset (U32 ($names + 4 * $i))
    $end = $o
    while ($b[$end] -ne 0) { $end++ }
    [Text.Encoding]::ASCII.GetString($b, $o, $end - $o)
}
$actual = @($actual | Sort-Object)
$diff = Compare-Object $expected $actual
if ($diff) {
    $diff | Format-Table | Out-String | Write-Host
    throw "$Path exports differ from the mIRC interface"
}
Write-Host "$(Split-Path $Path -Leaf) ($Bits-bit): $($actual -join ', ')"
