<#
.SYNOPSIS
Runs the storage branch's release foldbench on the whole-Bible corpus and writes the results as
Markdown, for the Windows reference laptop (PERFORMANCE.md, "Budgets").

.DESCRIPTION
It does on Windows what docs/evidence/storage-fold-2026-10-16.md (on the storage branch) does
on Linux: one open per process, interleaved rounds. foldbench reads CPU time and memory from
Linux's /proc, so on Windows it prints them as "unavailable"; this script measures them from
outside instead: CPU time, peak working set and peak private bytes of each foldbench process,
and its working set at the moment foldbench reports the fold open (the steady figure).

Phase warm (the default) builds foldbench, generates the corpus once, runs the rounds with the
files in the file cache, runs every part once in one process, and keeps a second copy of the
corpus without its fold cache for the cold phase. Phase cold runs after a reboot, before
anything else reads the corpus: one open with the fold cache and one first open without it,
each on files no process has read since the reboot. It adds its rows to the same report.

Works in Windows PowerShell 5.1 and in PowerShell 7 (Windows and Linux). Records no token and
no password.

.EXAMPLE
powershell -ExecutionPolicy Bypass -File spikes\foldbench-windows\run-foldbench.ps1 -Storage C:\tc4-g0\storage
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Storage,
    [string]$Corpus = '',
    [string]$Report = '',
    [ValidateSet('warm', 'cold')][string]$Phase = 'warm',
    [int]$Rounds = 5,
    [string]$Spec = 'bible',
    [int]$Threads = 4,
    [switch]$SkipBuild,
    [switch]$SkipEverything
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '../machine.ps1')

$onWindows = Test-OnWindows
$repo = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$Storage = (Resolve-Path $Storage).Path
$started = (Get-Date).ToUniversalTime()
$stamp = $started.ToString('yyyy-MM-dd', [Globalization.CultureInfo]::InvariantCulture)
if (-not $Report) { $Report = Join-Path $repo "docs/evidence/storage-fold-windows-$stamp.md" }
if (-not $Corpus) { $Corpus = Join-Path (Split-Path -Parent $Storage) 'tc4-g0-corpus' }
$Report = Resolve-FullPath $Report
$Corpus = Resolve-FullPath $Corpus
$coldCopy = "$Corpus-nocache"
$exe = if ($onWindows) { 'foldbench.exe' } else { 'foldbench' }
$target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $Storage 'target' }
$foldbench = Join-Path $target "release/examples/$exe"
$raw = Join-Path (Split-Path -Parent $Report) ([IO.Path]::GetFileNameWithoutExtension($Report))
New-Item -ItemType Directory -Force -Path $raw | Out-Null
$rowsFile = Join-Path $raw "rows-$Phase.tsv"
$foldbenchVariables = @('FOLDBENCH_RUNS', 'FOLDBENCH_FIRST_ONLY', 'FOLDBENCH_CACHED_ONLY', 'FOLDBENCH_STREAM_ONLY',
    'FOLDBENCH_FOLD_ONLY', 'FOLDBENCH_PSALMS_ONLY', 'FOLDBENCH_NO_O7', 'FOLDBENCH_PROJECTIONS', 'FOLDBENCH_DUMP')

function Say([string]$Text) {
    Write-Host ("[{0}] {1}" -f (Get-Date).ToString('HH:mm:ss'), $Text)
}

function Fmt([object]$Value, [string]$Format = '{0:0}') {
    if ($null -eq $Value) { return 'unavailable' }
    return Format-Invariant $Format ([double]$Value)
}

function MiB([object]$Bytes) {
    if ($null -eq $Bytes -or [double]$Bytes -le 0) { return $null }
    return [double]$Bytes / 1MB
}

# A Rust Duration as foldbench prints it with {:?} ("1.634s", "499.1ms", "812us" with a micro
# sign), in ms. The script stays ASCII because Windows PowerShell 5.1 reads it as ANSI.
function ConvertFrom-Duration([string]$Text) {
    $m = [regex]::Match($Text, '^([0-9.]+)(ns|\u00B5s|us|ms|s)$')
    if (-not $m.Success) { return $null }
    $value = [double]::Parse($m.Groups[1].Value, [Globalization.CultureInfo]::InvariantCulture)
    switch ($m.Groups[2].Value -replace '\u00B5', 'u') {
        'ns' { return $value / 1e6 }
        'us' { return $value / 1e3 }
        'ms' { return $value }
        's' { return $value * 1e3 }
    }
}

function Get-Median([double[]]$Values) {
    if ($Values.Count -eq 0) { return $null }
    $sorted = @($Values | Sort-Object)
    return $sorted[[math]::Floor($sorted.Count / 2)]
}

function Get-P95([double[]]$Values) {
    if ($Values.Count -eq 0) { return $null }
    $sorted = @($Values | Sort-Object)
    return $sorted[[math]::Min($sorted.Count - 1, [math]::Floor($sorted.Count * 0.95))]
}

function Get-Range([object[]]$Values, [string]$Unit, [string]$Format = '{0:0}') {
    $numbers = @($Values | Where-Object { $null -ne $_ } | ForEach-Object { [double]$_ })
    if ($numbers.Count -eq 0) { return 'unavailable' }
    $low = ($numbers | Measure-Object -Minimum).Minimum
    $high = ($numbers | Measure-Object -Maximum).Maximum
    if ((Fmt $low $Format) -eq (Fmt $high $Format)) { return "$(Fmt $low $Format) $Unit" }
    return "$(Fmt $low $Format)-$(Fmt $high $Format) $Unit"
}

function With-Unit([object]$Value, [string]$Unit, [string]$Format = '{0:0}') {
    if ($null -eq $Value) { return 'unavailable' }
    return "$(Fmt $Value $Format) $Unit"
}

# Reads every file of the corpus once, so the next open finds them in the file cache.
function Read-Corpus([string]$Dir) {
    $buffer = New-Object byte[] (1MB)
    foreach ($file in (Get-ChildItem -LiteralPath $Dir -Recurse -File)) {
        $stream = [IO.File]::OpenRead($file.FullName)
        try { while ($stream.Read($buffer, 0, $buffer.Length) -gt 0) { } } finally { $stream.Dispose() }
    }
}

# One reading of a running process; $null once it has exited. A figure the platform does not
# give (peak private bytes on Linux) is $null on its own.
function Get-Sample([System.Diagnostics.Process]$Process) {
    try {
        $Process.Refresh()
        if ($Process.HasExited) { return $null }
        $workingSet = $Process.WorkingSet64
    } catch {
        return $null
    }
    # Linux reads an exited, unreaped process as a working set of 0.
    if ($workingSet -le 0) { return $null }
    $read = {
        param($get)
        try { $value = & $get; if ($value -gt 0) { return $value } } catch { }
        return $null
    }
    return [pscustomobject]@{
        WorkingSet     = $workingSet
        PeakWorkingSet = & $read { $Process.PeakWorkingSet64 }
        Private        = & $read { $Process.PrivateMemorySize64 }
        PeakPrivate    = & $read { $Process.PeakPagedMemorySize64 }
        Cpu            = & $read { $Process.TotalProcessorTime.TotalMilliseconds }
    }
}

# Runs foldbench once and measures it from outside: it samples the process every 20 ms while
# reading its output, and at each watched stdout line it keeps a reading taken right then (or,
# if the process has already exited, the last one before). Peaks are the operating system's
# own, so a sample at any time after the peak sees it.
function Invoke-Foldbench([string[]]$Arguments, [hashtable]$Variables, [string[]]$Watch) {
    $info = New-Object System.Diagnostics.ProcessStartInfo
    $info.FileName = $foldbench
    $info.Arguments = Join-Arguments $Arguments
    $info.UseShellExecute = $false
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $info.CreateNoWindow = $true
    $info.StandardOutputEncoding = [Text.Encoding]::UTF8
    $info.StandardErrorEncoding = [Text.Encoding]::UTF8
    foreach ($name in $foldbenchVariables) {
        if ($info.EnvironmentVariables.ContainsKey($name)) { $info.EnvironmentVariables.Remove($name) }
    }
    if ($Variables) { foreach ($name in $Variables.Keys) { $info.EnvironmentVariables[$name] = [string]$Variables[$name] } }
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $process = [System.Diagnostics.Process]::Start($info)
    $stderrTask = $process.StandardError.ReadToEndAsync()
    $lines = New-Object System.Collections.Generic.List[string]
    $marks = @{}
    $last = $null
    $cpuSeen = $null
    $reading = $process.StandardOutput.ReadLineAsync()
    while ($true) {
        if ($reading.IsCompleted) {
            $line = $reading.Result
            if ($null -eq $line) { break }
            $lines.Add($line)
            $now = Get-Sample $process
            if ($now) { $last = $now; if ($now.Cpu) { $cpuSeen = $now.Cpu } }
            foreach ($prefix in $Watch) {
                if ($line.StartsWith($prefix) -and -not $marks.ContainsKey($prefix) -and $last) { $marks[$prefix] = $last }
            }
            $reading = $process.StandardOutput.ReadLineAsync()
            continue
        }
        $now = Get-Sample $process
        if ($now) { $last = $now; if ($now.Cpu) { $cpuSeen = $now.Cpu } }
        Start-Sleep -Milliseconds 20
    }
    $process.WaitForExit()
    $clock.Stop()
    # Windows keeps an exited process's times; elsewhere the last reading is the best there is.
    $cpu = $null
    try { $cpu = $process.TotalProcessorTime.TotalMilliseconds } catch { $cpu = $null }
    if (-not $cpu) { $cpu = $cpuSeen }
    $stderr = $stderrTask.Result
    # A foldbench from before the storage branch's 225091b prints the figures it reads from
    # Linux's /proc as 0 where there is no /proc; a newer one prints "unavailable".
    if (-not (Test-Path '/proc/self/status') -and ($lines -join "`n") -match ' 0 MiB') { $script:procZeros = $true }
    if ($process.ExitCode -ne 0) {
        throw "foldbench $($Arguments -join ' ') failed with exit code $($process.ExitCode): $stderr"
    }
    return [pscustomobject]@{
        Stdout         = $lines.ToArray()
        Stderr         = $stderr
        WallMs         = $clock.Elapsed.TotalMilliseconds
        CpuMs          = $cpu
        PeakWorkingSet = if ($last) { $last.PeakWorkingSet } else { $null }
        PeakPrivate    = if ($last) { $last.PeakPrivate } else { $null }
        Marks          = $marks
    }
}

function Find-Line([string[]]$Lines, [string]$Pattern) {
    foreach ($line in $Lines) {
        $m = [regex]::Match($line, $Pattern)
        if ($m.Success) { return $m }
    }
    return $null
}

function Add-Row([string]$Round, [string]$Mode, [object]$Row) {
    if (-not (Test-Path -LiteralPath $rowsFile)) {
        Set-Content -LiteralPath $rowsFile -Value "round`tmode`twall_ms`tafter_last_file_ms`tcpu_ms`tpeak_ws_mib`tpeak_private_mib`tws_fold_open_mib`tprocess_wall_ms"
    }
    $fields = @($Round, $Mode, (Fmt $Row.Wall), (Fmt $Row.Tail), (Fmt $Row.Cpu), (Fmt (MiB $Row.PeakWorkingSet)),
        (Fmt (MiB $Row.PeakPrivate)), (Fmt (MiB $Row.Steady)), (Fmt $Row.ProcessWall))
    Add-Content -LiteralPath $rowsFile -Value ($fields -join "`t")
}

# (c) The first open without a cache: foldbench writes the cache after it.
function Invoke-FirstOpen([string]$Dir) {
    $run = Invoke-Foldbench @('run', $Dir, "$Threads") @{ FOLDBENCH_FIRST_ONLY = '1'; FOLDBENCH_RUNS = '1' } @('first open without a cache')
    $m = Find-Line $run.Stdout 'first open without a cache (\d+) ms, the fold after the last file (\d+) ms'
    if (-not $m) { throw "foldbench printed no first-open line: $($run.Stdout -join ' / ')" }
    $mark = $run.Marks['first open without a cache']
    return [pscustomobject]@{
        Wall           = [double]$m.Groups[1].Value
        Tail           = [double]$m.Groups[2].Value
        Cpu            = $run.CpuMs
        PeakWorkingSet = if ($mark) { $mark.PeakWorkingSet } else { $run.PeakWorkingSet }
        PeakPrivate    = $run.PeakPrivate
        Steady         = $null
        ProcessWall    = $run.WallMs
        Machine        = $run.Stdout | Where-Object { $_ -like 'cpu:*' } | Select-Object -First 1
    }
}

# (a) An open with the fold cache; the working set when foldbench reports the fold open.
function Invoke-CachedOpen([string]$Dir) {
    $run = Invoke-Foldbench @('run', $Dir, "$Threads") @{ FOLDBENCH_CACHED_ONLY = '1'; FOLDBENCH_RUNS = '1' } @('fold cache:')
    if ($run.Stderr -match 'not used') { throw "foldbench did not use the cache: $($run.Stderr)" }
    $m = [regex]::Match($run.Stderr, 'cold open with the cache (\S+), the fold after the last file (\S+),')
    if (-not $m.Success) { throw "foldbench printed no open line: $($run.Stderr)" }
    $mark = $run.Marks['fold cache:']
    return [pscustomobject]@{
        Wall           = ConvertFrom-Duration $m.Groups[1].Value
        Tail           = ConvertFrom-Duration $m.Groups[2].Value
        Cpu            = $run.CpuMs
        PeakWorkingSet = if ($mark) { $mark.PeakWorkingSet } else { $run.PeakWorkingSet }
        PeakPrivate    = $run.PeakPrivate
        Steady         = if ($mark) { $mark.WorkingSet } else { $null }
        ProcessWall    = $run.WallMs
        Machine        = $run.Stdout | Where-Object { $_ -like 'cpu:*' } | Select-Object -First 1
    }
}

# The whole Bible open with the cache, then Psalms projected 20 times (FOLDBENCH_PSALMS_ONLY),
# without the steps `foldbench app` adds; the app's case for memory is Invoke-App.
function Invoke-Psalms([string]$Dir) {
    $watch = @('RSS with the fold open', 'peak RSS while Psalms was projected')
    $run = Invoke-Foldbench @('run', $Dir, "$Threads") @{ FOLDBENCH_PSALMS_ONLY = '1'; FOLDBENCH_RUNS = '1' } $watch
    $projection = Find-Line $run.Stdout '^\| Psalms projection: [^|]*\| (\d+) \| ([0-9.]+) ms \| ([0-9.]+) ms \|'
    $usfm = Find-Line $run.Stdout '^\| Psalms USFM alone \| (\d+) \| ([0-9.]+) ms \| ([0-9.]+) ms \|'
    $chapter = Find-Line $run.Stdout "^\| One Psalms chapter's USFM \| (\d+) \| ([0-9.]+) ms \| ([0-9.]+) ms \|"
    if (-not $projection) { throw "foldbench printed no Psalms projection row: $($run.Stdout -join ' / ')" }
    $before = $run.Marks['RSS with the fold open']
    $after = $run.Marks['peak RSS while Psalms was projected']
    $parse = { param($m, $i) if ($m) { [double]::Parse($m.Groups[$i].Value, [Globalization.CultureInfo]::InvariantCulture) } else { $null } }
    return [pscustomobject]@{
        Runs          = [int]$projection.Groups[1].Value
        Median        = & $parse $projection 2
        P95           = & $parse $projection 3
        UsfmMedian    = & $parse $usfm 2
        ChapterMedian = & $parse $chapter 2
        SteadyBefore  = if ($before) { $before.WorkingSet } else { $null }
        PeakAfter     = if ($after) { $after.PeakWorkingSet } else { $run.PeakWorkingSet }
        SteadyAfter   = if ($after) { $after.WorkingSet } else { $null }
        PeakPrivate   = $run.PeakPrivate
        Lines         = $run.Stdout
    }
}

# The app's case, the one the storage branch takes its memory budgets from: `foldbench app`
# opens the project once with the fold cache in a process that has done nothing else, then a
# person works in Psalms (saves, decisions, alignments, conflict lists, the book's projection,
# snapshots). Measured from outside, like the rest; $null with the reason if this foldbench
# has no `app`.
function Invoke-App([string]$Dir) {
    $watch = @('open with the fold cache', 'peak RSS of the incremental steps')
    try {
        $run = Invoke-Foldbench @('app', $Dir, "$Threads") @{} $watch
    } catch {
        return [pscustomobject]@{ Error = $_.Exception.Message }
    }
    $open = $run.Marks['open with the fold cache']
    if (-not $open) {
        return [pscustomobject]@{ Error = "foldbench app printed no open with the fold cache: $($run.Stdout -join ' / ')" }
    }
    $end = $run.Marks['peak RSS of the incremental steps']
    return [pscustomobject]@{
        Error       = $null
        OpenSteady  = $open.WorkingSet
        OpenPeak    = $open.PeakWorkingSet
        EndSteady   = if ($end) { $end.WorkingSet } else { $null }
        Peak        = if ($end) { $end.PeakWorkingSet } else { $run.PeakWorkingSet }
        PeakPrivate = $run.PeakPrivate
        Cpu         = $run.CpuMs
        Wall        = $run.WallMs
        Lines       = $run.Stdout
    }
}

# foldbench's table rows; it prints no header before the app's steps, so one is added.
function Add-TableRows([string[]]$Lines) {
    $rows = @($Lines | Where-Object { $_.StartsWith('|') })
    if ($rows.Count -eq 0) { return }
    if (-not $rows[0].StartsWith('| Action')) {
        Add '| Action | Runs | Median | p95 |'
        Add '|---|---:|---:|---:|'
    }
    foreach ($row in $rows) { Add $row }
}

function Test-Corpus([string]$Dir) {
    return (Test-Path -LiteralPath (Join-Path $Dir 'project')) -and
    (@(Get-ChildItem -LiteralPath (Join-Path $Dir 'packs') -File -ErrorAction SilentlyContinue).Count -gt 0)
}

function Get-CorpusSize([string]$Dir) {
    $packs = @(Get-ChildItem -LiteralPath (Join-Path $Dir 'packs') -File)
    $pending = @(Get-ChildItem -LiteralPath (Join-Path $Dir 'pending') -File)
    $bytes = (($packs + $pending) | Measure-Object -Property Length -Sum).Sum
    return "$($packs.Count) packs and $($pending.Count) pending files, $(Format-Invariant '{0:N0}' $bytes) bytes"
}

$script:procZeros = $false
$machineFacts = Get-MachineFacts ''
$storageCommit = Get-Commit $Storage
$scriptCommit = Get-Commit $repo
$md = New-Object System.Collections.Generic.List[string]
function Add([string]$Line) { $md.Add($Line) }

# ------------------------------------------------------------------ phase cold

if ($Phase -eq 'cold') {
    if (-not (Test-Path -LiteralPath $Report)) { throw "run the warm phase first; $Report does not exist" }
    if (-not (Test-Path -LiteralPath $foldbench)) { throw "no foldbench at $foldbench; run the warm phase first" }
    if (-not (Test-Corpus $Corpus) -or -not (Test-Corpus $coldCopy)) { throw "the corpus or its copy is missing; run the warm phase first" }
    $boot = 'unavailable'
    if ($onWindows) {
        $since = (Get-Date) - (Get-CimInstance Win32_OperatingSystem).LastBootUpTime
        $boot = Format-Invariant '{0:0} minutes' $since.TotalMinutes
    }
    Say "Cold phase, $boot after the last boot: one open with the fold cache"
    $cached = Invoke-CachedOpen $Corpus
    Add-Row 'cold' 'a_cache_cold' $cached
    Say 'Cold phase: one first open without a cache, on the copy'
    # An earlier cold run left its fold cache in the copy; removing it reads no corpus file.
    $copyIndex = Join-Path $coldCopy 'index.bin'
    if (Test-Path -LiteralPath $copyIndex) { Remove-Item -LiteralPath $copyIndex -Force }
    $first = Invoke-FirstOpen $coldCopy
    Add-Row 'cold' 'c_nocache_cold' $first
    Add ''
    Add "## After a reboot ($stamp)"
    Add ''
    Add "Run $boot after the last boot, before anything else read the corpus: the open with the"
    Add "fold cache read ``$(Split-Path -Leaf $Corpus)``, the first open without a cache read its copy"
    Add "``$(Split-Path -Leaf $coldCopy)``. One run each, so no median."
    Add ''
    Add '| Action | Wall | The fold after the last file | CPU, all threads | Peak working set | Peak private bytes | Working set with the fold open | Budget |'
    Add '|---|---:|---:|---:|---:|---:|---:|---|'
    Add "| (b) Open with the fold cache, after a reboot | $(With-Unit $cached.Wall 'ms') | $(With-Unit $cached.Tail 'ms') | $(With-Unit $cached.Cpu 'ms') | $(With-Unit (MiB $cached.PeakWorkingSet) 'MiB') | $(With-Unit (MiB $cached.PeakPrivate) 'MiB') | $(With-Unit (MiB $cached.Steady) 'MiB') | 10 s for the first open after a restart |"
    Add "| (c) First open without a cache, after a reboot | $(With-Unit $first.Wall 'ms') | $(With-Unit $first.Tail 'ms') | $(With-Unit $first.Cpu 'ms') | $(With-Unit (MiB $first.PeakWorkingSet) 'MiB') | $(With-Unit (MiB $first.PeakPrivate) 'MiB') | - | 10 s |"
    Add ''
    $existing = [IO.File]::ReadAllText($Report)
    Write-Utf8File $Report ($existing.TrimEnd("`n") + "`n" + ($md -join "`n") + "`n")
    Say "Added the cold rows to $Report"
    exit 0
}

# ------------------------------------------------------------------ phase warm: build and corpus

$buildNote = 'skipped (-SkipBuild)'
if (-not $SkipBuild) {
    Say 'Building foldbench (cargo build --release -p tc-corpus --example foldbench); this takes several minutes'
    $clock = [Diagnostics.Stopwatch]::StartNew()
    Push-Location $Storage
    try {
        & cargo build --release -p tc-corpus --example foldbench
        if ($LASTEXITCODE -ne 0) { throw 'cargo build failed; see the output above' }
    } finally { Pop-Location }
    $buildNote = Format-Invariant 'built in {0:0.0} min' $clock.Elapsed.TotalMinutes
}
if (-not (Test-Path -LiteralPath $foldbench)) { throw "no foldbench at $foldbench" }
$toolchain = ((& rustc --version) -join ' ')

$generated = 'reused from an earlier run'
if (-not (Test-Corpus $Corpus)) {
    Say "Generating the corpus ($Spec) in $Corpus; about a minute"
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $specArgs = @($Spec -split ' ')
    $info = Invoke-Foldbench (@('save') + $specArgs + @($Corpus)) @{} @()
    $generated = (Format-Invariant 'generated in {0:0.0} s' $clock.Elapsed.TotalSeconds) + "; $($info.Stderr.Trim())"
}
$corpusSize = Get-CorpusSize $Corpus
$index = Join-Path $Corpus 'index.bin'

if (-not (Test-Corpus $coldCopy)) {
    Say "Copying the corpus without its fold cache to $coldCopy, for the cold phase"
    New-Item -ItemType Directory -Force -Path $coldCopy | Out-Null
    foreach ($part in @('packs', 'pending')) {
        Copy-Item -LiteralPath (Join-Path $Corpus $part) -Destination $coldCopy -Recurse -Force
    }
    Copy-Item -LiteralPath (Join-Path $Corpus 'project') -Destination $coldCopy -Force
}

# ------------------------------------------------------------------ phase warm: rounds

$firsts = @()
$cacheds = @()
$psalms = @()
$apps = @()
$machineLine = $null
for ($round = 1; $round -le $Rounds; $round++) {
    Say "Round $round of ${Rounds}: the first open without a cache"
    if (Test-Path -LiteralPath $index) { Remove-Item -LiteralPath $index -Force }
    Read-Corpus $Corpus
    $first = Invoke-FirstOpen $Corpus
    if (-not $machineLine) { $machineLine = $first.Machine }
    $firsts += $first
    Add-Row "$round" 'c_nocache_warm' $first

    Say "Round $round of ${Rounds}: an open with the fold cache"
    Read-Corpus $Corpus
    $cached = Invoke-CachedOpen $Corpus
    $cacheds += $cached
    Add-Row "$round" 'a_cache_warm' $cached

    Say "Round $round of ${Rounds}: the whole Bible open, Psalms projected 20 times"
    Read-Corpus $Corpus
    $p = Invoke-Psalms $Corpus
    $psalms += $p
    Add-Content -LiteralPath $rowsFile -Value ("$round`tpsalms`tmedian $(Fmt $p.Median '{0:0.0}')`tp95 $(Fmt $p.P95 '{0:0.0}')`tws_before $(Fmt (MiB $p.SteadyBefore))`tpeak $(Fmt (MiB $p.PeakAfter))`tws_after $(Fmt (MiB $p.SteadyAfter))")

    Say "Round $round of ${Rounds}: the app's case (foldbench app): one open, then work in Psalms"
    Read-Corpus $Corpus
    $a = Invoke-App $Corpus
    $apps += $a
    if ($a.Error) {
        Say "  the app's case did not run: $($a.Error)"
        Add-Content -LiteralPath $rowsFile -Value "$round`tapp`terror`t$($a.Error -replace '\s+', ' ')"
    } else {
        Set-Content -LiteralPath (Join-Path $raw "app-$round.txt") -Value $a.Lines
        Add-Content -LiteralPath $rowsFile -Value ("$round`tapp`tws_open $(Fmt (MiB $a.OpenSteady))`tpeak_open $(Fmt (MiB $a.OpenPeak))`tpeak $(Fmt (MiB $a.Peak))`tws_end $(Fmt (MiB $a.EndSteady))`tpeak_private $(Fmt (MiB $a.PeakPrivate))")
    }
    Start-Sleep -Seconds 2
}

$everything = $null
if (-not $SkipEverything) {
    Say 'Every part once in one process (foldbench run); a few minutes'
    Read-Corpus $Corpus
    $everything = Invoke-Foldbench @('run', $Corpus, "$Threads") @{} @()
    $note = @()
    if ($script:procZeros) {
        $note = @('NOTE: this foldbench prints the CPU and memory figures it reads from Linux''s /proc as 0 on this system. They are not measurements.', '')
    }
    Set-Content -LiteralPath (Join-Path $raw 'everything.txt') -Value ($note + $everything.Stdout + @('', '--- stderr ---', $everything.Stderr))
}

# ------------------------------------------------------------------ the report

$machineFacts = Get-MachineFacts (ConvertFrom-MachineLine $machineLine)
$finished = (Get-Date).ToUniversalTime()
$platform = if ($onWindows) { 'Windows' } else { 'Linux' }
Add "# The fold at Bible scale on $platform ($stamp)"
Add ''
Add 'Written by `spikes/foldbench-windows/run-foldbench.ps1` (unit G0, docs/APP-DESIGN.md section 10). It'
Add 'runs the release `foldbench` of `tc-corpus` from the storage branch on the corpus of STORAGE.md'
Add 'section 14 as `docs/evidence/storage-fold-2026-10-16.md` on that branch does on Linux: one open per'
Add 'process, the rounds interleaved. Only O8 of section 6.1 is measured, so the 3 s cold open is not shown'
Add 'end to end (it needs `tc-store`).'
Add ''
Add 'foldbench reads CPU time and memory from Linux''s `/proc`; on Windows it prints them as'
Add '"unavailable". This script measures them from outside: the CPU time of the whole foldbench'
Add 'process, its peak working set (the Windows counterpart of peak RSS), its peak private bytes,'
Add 'and its working set when foldbench reports the fold open (the steady figure). Each process'
Add 'also computes the probe stamp once (about 5 ms), which its CPU time includes.'
Add ''
Add "- Storage commit: $storageCommit"
Add "- Script commit: $scriptCommit"
Add "- Build: $toolchain, release profile; $buildNote"
Add "- Corpus (``$Spec``): $corpusSize; $generated"
Add ("- Run: {0} to {1} UTC; {2} rounds, {3} reading threads" -f $started.ToString('yyyy-MM-dd HH:mm'), $finished.ToString('HH:mm'), $Rounds, $Threads)
Add "- foldbench's machine line: ``$machineLine``"
if ($script:procZeros) {
    Add '- This foldbench prints the figures it reads from Linux''s `/proc` as 0 here (the storage branch'
    Add '  prints "unavailable" from `225091b` on); none of those zeros is used below.'
}
Add ('- Raw rows: `{0}`' -f ($raw.Replace($repo, '.').Replace('\', '/')))
Add ''
Add '## Machine'
Add ''
foreach ($line in (Format-MachineTable $machineFacts)) { Add $line }
Add ''
Add '## Load and fold of the whole Bible'
Add ''
Add 'Warm: every file was read once just before each open, so it was in the file cache. Times in'
Add 'ms; the wall time is foldbench''s own, CPU and memory are measured from outside.'
Add ''
Add '| Action | Runs | Median | p95 | The fold after the last file (median) | CPU, all threads (median) | Peak working set | Peak private bytes | Working set with the fold open | Budget |'
Add '|---|---:|---:|---:|---:|---:|---:|---:|---:|---|'
$rows = @(
    @{ Name = '(a) Open with the fold cache, warm'; Runs = $cacheds; Budget = '3 s' },
    @{ Name = '(c) First open without a cache, warm'; Runs = $firsts; Budget = '10 s' }
)
foreach ($row in $rows) {
    $runs = @($row.Runs)
    $walls = @($runs | ForEach-Object { [double]$_.Wall })
    $tails = @($runs | ForEach-Object { [double]$_.Tail })
    $cpus = @($runs | Where-Object { $null -ne $_.Cpu } | ForEach-Object { [double]$_.Cpu })
    $steady = Get-Range @($runs | ForEach-Object { MiB $_.Steady }) 'MiB'
    if ($row.Name -like '(c)*') { $steady = '-' }
    Add ("| $($row.Name) | $($runs.Count) | $(Fmt (Get-Median $walls)) | $(Fmt (Get-P95 $walls)) | $(Fmt (Get-Median $tails)) | $(Fmt (Get-Median $cpus)) | " +
        "$(Get-Range @($runs | ForEach-Object { MiB $_.PeakWorkingSet }) 'MiB') | $(Get-Range @($runs | ForEach-Object { MiB $_.PeakPrivate }) 'MiB') | $steady | $($row.Budget) |")
}
Add ''
$appsRan = @($apps | Where-Object { -not $_.Error })
Add '## The app''s case for memory'
Add ''
Add 'Each round runs `foldbench app` in a process of its own: one open of the whole Bible with the'
Add 'fold cache, then a person working in Psalms (200 saves of a verse, decisions, alignments and'
Add 'resolutions, the conflict lists, the book''s projection and 20 snapshots). This is the case'
Add 'the storage branch takes the memory budgets from. Working sets are read when foldbench'
Add 'reports the open and when it reports the end of the steps.'
Add ''
if ($appsRan.Count -gt 0) {
    Add '| Figure | Value over the processes | Budget |'
    Add '|---|---|---|'
    Add "| Working set with the fold open, before the steps | $(Get-Range @($appsRan | ForEach-Object { MiB $_.OpenSteady }) 'MiB') | 150 MiB steady |"
    Add "| Peak working set of the open | $(Get-Range @($appsRan | ForEach-Object { MiB $_.OpenPeak }) 'MiB') | 350 MiB peak |"
    Add "| Peak working set of the process, the open and the steps | $(Get-Range @($appsRan | ForEach-Object { MiB $_.Peak }) 'MiB') | 350 MiB peak |"
    Add "| Working set after the steps | $(Get-Range @($appsRan | ForEach-Object { MiB $_.EndSteady }) 'MiB') | 150 MiB steady |"
    Add "| Peak private bytes | $(Get-Range @($appsRan | ForEach-Object { MiB $_.PeakPrivate }) 'MiB') | |"
    Add "| CPU time of the process | $(Get-Range @($appsRan | ForEach-Object { $_.Cpu }) 'ms') | |"
    Add ''
    Add 'foldbench''s own step times from the first round (its memory figures read "unavailable" on'
    Add 'Windows; the table above measures them):'
    Add ''
    Add-TableRows $appsRan[0].Lines
} else {
    Add "**The app's case did not run**: $(($apps | Select-Object -First 1).Error)"
}
Add ''
Add '## Psalms in use, without the app''s steps'
Add ''
Add 'Each process opens the whole Bible with the fold cache, then projects every file of Psalms 20'
Add 'times with the values read from disk (what materializing the book renders), and stops'
Add '(`FOLDBENCH_PSALMS_ONLY`): the projection times, and memory without the steps `foldbench app`'
Add 'adds. The memory budgets apply to the app''s case above.'
Add ''
Add '| Figure | Value over the processes | Budget |'
Add '|---|---|---|'
Add "| Psalms projection, median of 20 | $(Get-Range @($psalms | ForEach-Object { $_.Median }) 'ms') | 300 ms |"
Add "| Psalms projection, p95 of 20 | $(Get-Range @($psalms | ForEach-Object { $_.P95 }) 'ms') | 300 ms |"
Add "| Psalms USFM alone, median | $(Get-Range @($psalms | ForEach-Object { $_.UsfmMedian }) 'ms' '{0:0.0}') | 150 ms (open a book) |"
Add "| One Psalms chapter's USFM, median | $(Get-Range @($psalms | ForEach-Object { $_.ChapterMedian }) 'ms' '{0:0.00}') | 50 ms (first chapter) |"
Add "| Working set with the whole Bible open, before Psalms | $(Get-Range @($psalms | ForEach-Object { MiB $_.SteadyBefore }) 'MiB') | see the app's case |"
Add "| Peak working set of the process, the open and the projections | $(Get-Range @($psalms | ForEach-Object { MiB $_.PeakAfter }) 'MiB') | see the app's case |"
Add "| Working set after Psalms was projected | $(Get-Range @($psalms | ForEach-Object { MiB $_.SteadyAfter }) 'MiB') | see the app's case |"
Add "| Peak private bytes | $(Get-Range @($psalms | ForEach-Object { MiB $_.PeakPrivate }) 'MiB') | |"
Add ''
Add 'Windows cannot reset a process''s peak, so the peak above covers the open and the projections'
Add 'together; when the projections peak lower than the open, this shows the open''s peak.'
Add ''
if ($everything) {
    # `foldbench run` ends by starting `foldbench app` as a child process that writes to the same
    # output; its lines start at this one and belong to the app's case, not to this process.
    $parent = @()
    $child = @()
    $inChild = $false
    foreach ($line in $everything.Stdout) {
        if ($line -like "the app's case, in a process of its own*") { $inChild = $true }
        if ($inChild) { $child += $line } else { $parent += $line }
    }
    Add '## Every part, one process'
    Add ''
    Add '`foldbench run CORPUS 4` with its defaults (5 runs of each open). Its memory and CPU columns'
    Add 'read "unavailable" on Windows. An open in a process that has already opened the project'
    Add 'several times is not the app''s case; the app''s case is the table above and the next part.'
    Add ''
    foreach ($line in $parent) {
        if ($line.StartsWith('|')) { Add $line }
    }
    Add ''
    if (-not $script:procZeros) {
        foreach ($line in $parent) {
            if ($line -like 'corpus:*' -or $line -like 'fold cache:*' -or $line -like 'Psalms:*') { Add "- $line" }
        }
        Add ''
    }
    if ($child.Count -gt 0) {
        Add '## The app''s case, as `foldbench run` starts it'
        Add ''
        Add '`foldbench run` ends by starting `foldbench app` as a child process. These are that'
        Add 'child''s rows; its memory is measured by the app''s-case table above, not here.'
        Add ''
        Add-TableRows $child
        Add ''
    }
}
Add '## What these results decide'
Add ''
Add '- The budgets in PERFORMANCE.md hold on the reference machine, a five-year-old Windows laptop'
Add '  with Defender on (owner ruling, 2026-10-16). These rows say, for the fold''s part, whether'
Add '  they hold there: the 3 s open with the cache, the 10 s open without one and after a restart,'
Add '  the 300 ms Psalms projection, and memory (350 MiB peak, 150 MiB steady).'
Add '- A CPU without SHA instructions hashes about 6.5 times slower (LOCAL-BENCH-FINDINGS F1); the'
Add '  machine line above says whether this one has them.'
Add '- The cold rows, if a reboot was possible, are added below by `-Phase cold`.'
Add ''

Write-Utf8File $Report (($md -join "`n") + "`n")
Say "Wrote $Report"
