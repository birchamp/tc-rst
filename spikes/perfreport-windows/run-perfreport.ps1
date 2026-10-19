<#
.SYNOPSIS
Runs unit P's performance report (crates/tc-corpus/examples/perfreport) on the Windows
reference laptop and writes the results as Markdown (PERFORMANCE.md rules 1 to 5).

.DESCRIPTION
perfreport measures every budget the storage core and the domain crates own, end to end on a
whole-Bible project: the table of docs/PERFORMANCE.md with the median and the 95th percentile of
each action and a verdict. On the laptop it is run without ratios: its figures are the laptop's
own, and the verdicts are the real ones. This script builds it, sets the project up, runs
the report in rounds (each action in a child process of its own, so one open per process),
then the saves-during-background-work cases (R9, tC4's J8), the memory cases and the growth
of memory with history, optionally the three simulated years, and writes everything it prints
beside a Markdown summary with the machine's facts (CPU, SHA and AVX-512 bits, disk, power
plan, Defender).

Memory on Windows is the process's private commit (PrivateUsage), which perfreport reads
itself through sysinfo; nothing is sampled from outside.

Files out of the page cache cannot be arranged without administrator rights on Windows: the
cold rows (a first open after a restart, the first load of Psalms resources, the first
launch's seeding) are cold only when the script runs right after a reboot, before anything
else reads the project. Run it twice: once after a reboot with -Only cold, which runs one
round of just those cases first, then the full run.

Works in Windows PowerShell 5.1 and in PowerShell 7. Records no token and no password.

.EXAMPLE
powershell -ExecutionPolicy Bypass -File spikes\perfreport-windows\run-perfreport.ps1 -Dir C:\tc4-perf
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Dir,
    [string]$Report = '',
    [int]$Rounds = 3,
    [ValidateSet('all', 'cold')][string]$Only = 'all',
    [switch]$Years,
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '../machine.ps1')

$repo = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
New-Item -ItemType Directory -Force -Path $Dir | Out-Null
$Dir = (Resolve-Path $Dir).Path
$started = (Get-Date).ToUniversalTime()
$stamp = $started.ToString('yyyy-MM-dd', [Globalization.CultureInfo]::InvariantCulture)
if (-not $Report) { $Report = Join-Path $repo "docs/evidence/storage-scale-windows-$stamp.md" }
$Report = Resolve-FullPath $Report
$raw = Join-Path (Split-Path -Parent $Report) ([IO.Path]::GetFileNameWithoutExtension($Report))
New-Item -ItemType Directory -Force -Path $raw | Out-Null
$onWindows = Test-OnWindows
$exe = if ($onWindows) { 'perfreport.exe' } else { 'perfreport' }
$target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $repo 'target' }
$perfreport = Join-Path $target "release/examples/$exe"

function Say([string]$Text) {
    Write-Host ("[{0}] {1}" -f (Get-Date).ToString('HH:mm:ss'), $Text)
}

# Runs perfreport with the arguments, copies its output to the console and to a file, and
# returns the lines. A non-zero exit code is reported, not fatal: a row over its limit exits
# with 1 and the report still has to be written.
function Invoke-Perfreport([string]$Name, [string[]]$Arguments) {
    $file = Join-Path $raw "$Name.txt"
    Say "perfreport $($Arguments -join ' ')"
    $lines = New-Object System.Collections.Generic.List[string]
    # Windows PowerShell 5.1 turns a native command's stderr into error records, which stop
    # the script under 'Stop'; perfreport writes its children's progress to stderr.
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        & $perfreport @Arguments 2>&1 | ForEach-Object {
            $line = "$_"
            $lines.Add($line)
            Write-Host $line
        }
        $code = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previous
    }
    Set-Content -LiteralPath $file -Value $lines.ToArray() -Encoding UTF8
    if ($code -ne 0) { Say "  exit code $code (rows over their limit are listed at the end of the output)" }
    return [pscustomobject]@{ Name = $Name; Lines = $lines.ToArray(); Code = $code }
}

if (-not $SkipBuild) {
    Say 'Building perfreport (cargo build --release -p tc-corpus --example perfreport); this takes several minutes'
    Push-Location $repo
    try {
        & cargo build --release -p tc-corpus --example perfreport
        if ($LASTEXITCODE -ne 0) { throw 'cargo build failed; see the output above' }
    } finally { Pop-Location }
}
if (-not (Test-Path -LiteralPath $perfreport)) { throw "no perfreport at $perfreport" }

$machineFacts = Get-MachineFacts ''
$commit = Get-Commit $repo
$toolchain = ((& rustc --version) -join ' ')
$runs = New-Object System.Collections.Generic.List[object]

if ($Only -eq 'cold') {
    # Right after a reboot: the first open after a restart, the first load of Psalms
    # resources and the first launch's seeding, on files nothing has read since.
    $runs.Add((Invoke-Perfreport 'cold-open' @('run', $Dir, '--only', 'open-cold,res-seed,res-load')))
} else {
    $runs.Add((Invoke-Perfreport 'setup' @('setup', $Dir)))
    $runs.Add((Invoke-Perfreport 'run' @('run', $Dir, '--rounds', "$Rounds")))
    $runs.Add((Invoke-Perfreport 'r9' @('r9', $Dir)))
    $runs.Add((Invoke-Perfreport 'memory' @('memory', $Dir)))
    $runs.Add((Invoke-Perfreport 'growth' @('growth', $Dir)))
    if ($Years) {
        $runs.Add((Invoke-Perfreport 'years' @('years', $Dir, '--open-every', '10')))
    }
}

# ---------------------------------------------------------------- the Markdown report

$md = New-Object System.Collections.Generic.List[string]
function Add([string]$Line) { $md.Add($Line) }

Add "# Unit P on the reference laptop ($stamp)"
Add ''
Add 'Written by `spikes/perfreport-windows/run-perfreport.ps1`. The figures are the laptop''s own:'
Add 'perfreport ran without ratios, so each verdict in the tables below is the laptop''s. The'
Add 'full output of each command is in the folder beside this file.'
Add ''
Add '| | |'
Add '|---|---|'
Add "| Commit | $commit |"
Add "| Toolchain | $toolchain |"
Add "| Rounds | $Rounds |"
Add "| Mode | $Only$(if ($Years) { ', with the three simulated years' }) |"
foreach ($key in $machineFacts.Keys) {
    Add "| $key | $("$($machineFacts[$key])" -replace '\|', '/') |"
}
Add ''
foreach ($run in $runs) {
    Add "## perfreport $($run.Name)"
    Add ''
    Add "Exit code $($run.Code); the full output is in ``$(Split-Path -Leaf $raw)/$($run.Name).txt``."
    Add ''
    # The table perfreport prints is the part to read; the machine line comes first.
    $table = @($run.Lines | Where-Object { $_ -notmatch '^\s*\[' -and $_ -notmatch '^ROW\s' })
    Add '```text'
    foreach ($line in $table) { Add $line }
    Add '```'
    Add ''
}
Write-Utf8File $Report (($md -join "`n") + "`n")
Say "Wrote $Report"
