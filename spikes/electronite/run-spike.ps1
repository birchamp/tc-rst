<#
.SYNOPSIS
Runs every case of the Electronite spike G0 and writes the results as Markdown.

.DESCRIPTION
Throwaway: the spike is never shipped. It needs the unpacked Electronite release (electron.exe)
and Rust (to build the stand-in server, tc4-standin). It works in Windows PowerShell 5.1 and in
PowerShell 7, on Windows and on Linux. It records no token and no password.

Cases, in this order:
  run       each mode: probe every request type, then 200 aligner clicks alone and 200 while
            OBS-sized pictures and a help page load, then the Graphite probe; every mode runs
            once per pass, in the opposite order in each pass after the first;
  restart   end the server; the shell starts it again and loads Home at the new address;
  close     close the window normally; the shell stops the server;
  endshell  end the shell's process; does the server exit within 5 s? Twice: the server
            started plainly, and started detached;
  hardkill  type until Chromium has written localStorage to the profile once and then for
            -AfterCommit seconds more, end the shell's process, then open the profile again
            and read what localStorage and the shell's mirror file kept; then the same with a
            normal quit, as the control.

.EXAMPLE
powershell -ExecutionPolicy Bypass -File spikes\electronite\run-spike.ps1 -Electron C:\tc4-g0\electronite\electron.exe
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Electron,
    [string]$Out = '',
    [int]$Clicks = 200,
    [int]$Warmup = 20,
    [int]$Passes = 2,
    [int]$AfterCommit = 10,
    [string]$Report = '',
    [string]$Server = '',
    [string[]]$Only = @()
)

$ErrorActionPreference = 'Stop'
# -Only run,close works from -File too, where PowerShell passes one string.
$Only = @($Only | ForEach-Object { $_ -split ',' } | ForEach-Object { $_.Trim() } | Where-Object { $_ })
foreach ($case in $Only) {
    if (@('run', 'restart', 'close', 'endshell', 'hardkill') -notcontains $case) {
        throw "-Only takes run, restart, close, endshell and hardkill, not '$case'"
    }
}
. (Join-Path $PSScriptRoot '../machine.ps1')

$onWindows = Test-OnWindows
$repo = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$started = (Get-Date).ToUniversalTime()
$stamp = $started.ToString('yyyy-MM-dd', [Globalization.CultureInfo]::InvariantCulture)
if (-not $Report) { $Report = Join-Path $repo "docs/evidence/electronite-spike-$stamp.md" }
if (-not $Out) { $Out = Join-Path $PSScriptRoot ('out/' + $started.ToString('yyyyMMdd-HHmmss', [Globalization.CultureInfo]::InvariantCulture)) }
$Report = Resolve-FullPath $Report
$Electron = (Resolve-Path $Electron).Path
$shellMain = Join-Path $PSScriptRoot 'shell/main.cjs'
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$Out = (Resolve-Path $Out).Path
$noSandbox = $false
if (-not $onWindows) {
    # Chromium refuses to run its sandbox as root on Linux (this container's tests).
    $noSandbox = ((id -u) -eq '0')
}

function Say([string]$Text) {
    Write-Host ("[{0}] {1}" -f (Get-Date).ToString('HH:mm:ss'), $Text)
}

function Want([string]$Case) {
    return ($Only.Count -eq 0) -or ($Only -contains $Case)
}

# ------------------------------------------------------------------ build the stand-in

if (-not $Server) {
    Say 'Building the stand-in server (cargo build --release)'
    $manifest = Join-Path $PSScriptRoot 'Cargo.toml'
    & cargo build --release --manifest-path $manifest
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed; see the output above' }
    $exe = if ($onWindows) { 'tc4-standin.exe' } else { 'tc4-standin' }
    $Server = Join-Path $PSScriptRoot "target/release/$exe"
}
$Server = (Resolve-Path $Server).Path
$cpuJson = (& $Server machine)
$machine = Get-MachineFacts $cpuJson
$commit = Get-Commit $repo

# ------------------------------------------------------------------ helpers

function Start-Shell([string]$Dir, [string]$Mode, [string]$Case, [hashtable]$Extra) {
    New-Item -ItemType Directory -Force -Path $Dir | Out-Null
    $argv = @()
    if ($noSandbox) { $argv += '--no-sandbox' }
    $argv += @($shellMain, "--spike-mode=$Mode", "--spike-case=$Case", "--spike-out=$Dir",
        "--spike-server=$Server", "--spike-clicks=$Clicks", "--spike-warmup=$Warmup")
    if ($Extra) { foreach ($key in $Extra.Keys) { $argv += "--spike-$key=$($Extra[$key])" } }
    $tag = Get-Tag $Mode $Case $Extra
    return Start-Process -FilePath $Electron -ArgumentList (Join-Arguments $argv) -PassThru -NoNewWindow `
        -RedirectStandardOutput (Join-Path $Dir "shell-$tag.out") -RedirectStandardError (Join-Path $Dir "shell-$tag.err")
}

function Get-Tag([string]$Mode, [string]$Case, [hashtable]$Extra) {
    $tag = "$Mode-$Case"
    if ($Extra -and $Extra.spawn -eq 'detached') { $tag += '-detached' }
    return $tag
}

function Read-Events([string]$Dir, [string]$Tag) {
    $file = Join-Path $Dir "events-$Tag.jsonl"
    if (-not (Test-Path -LiteralPath $file)) { return @() }
    $events = @()
    foreach ($line in (Get-Content -LiteralPath $file -Encoding UTF8)) {
        if (-not $line) { continue }
        try { $events += ($line | ConvertFrom-Json) } catch { }
    }
    return $events
}

function Wait-ShellEvent([string]$Dir, [string]$Tag, [string]$Name, [int]$Count, [int]$TimeoutMs) {
    $deadline = (Get-NowMs) + $TimeoutMs
    while ((Get-NowMs) -lt $deadline) {
        $found = @(Read-Events $Dir $Tag | Where-Object { $_.ev -eq $Name })
        if ($found.Count -ge $Count) { return $found[$Count - 1] }
        Start-Sleep -Milliseconds 50
    }
    return $null
}

function Read-Json([string]$File) {
    if (-not (Test-Path -LiteralPath $File)) { return $null }
    return (Get-Content -LiteralPath $File -Raw -Encoding UTF8 | ConvertFrom-Json)
}

function Test-Alive([int]$ProcessId) {
    return [bool](Get-Process -Id $ProcessId -ErrorAction SilentlyContinue)
}

# Milliseconds until the process is gone, or $null if it still runs after $TimeoutMs.
function Wait-Gone([int]$ProcessId, [long]$Since, [int]$TimeoutMs) {
    $deadline = $Since + $TimeoutMs
    while ((Get-NowMs) -lt $deadline) {
        if (-not (Test-Alive $ProcessId)) { return (Get-NowMs) - $Since }
        Start-Sleep -Milliseconds 10
    }
    return $null
}

function Get-ChildProcesses([int]$ParentId) {
    if ($onWindows) {
        return @(Get-CimInstance Win32_Process -Filter "ParentProcessId=$ParentId" |
                ForEach-Object { [pscustomobject]@{ Id = [int]$_.ProcessId; Name = $_.Name } })
    }
    $ids = @(ps -o pid= --ppid $ParentId 2>$null | ForEach-Object { $_.Trim() } | Where-Object { $_ })
    return @($ids | ForEach-Object {
            $name = (Get-Process -Id ([int]$_) -ErrorAction SilentlyContinue).ProcessName
            [pscustomobject]@{ Id = [int]$_; Name = $name }
        })
}

function Send-Control([string]$Dir, [string]$Tag, [string]$Command) {
    Set-Content -LiteralPath (Join-Path $Dir "control-$Tag.txt") -Value $Command
}

# Ends what a case started and left running. A process id can be reused once its process is
# gone, so only processes with the spike's names are ended.
function Stop-Leftovers([int[]]$Ids) {
    foreach ($id in $Ids) {
        if (-not $id) { continue }
        $process = Get-Process -Id $id -ErrorAction SilentlyContinue
        if ($process -and @('electron', 'tc4-standin', 'chrome_crashpad_handler') -contains $process.ProcessName) {
            Stop-Process -Id $id -Force -ErrorAction SilentlyContinue
        }
    }
}

# The last quit line the stand-in wrote in its app data, or why there is none.
function Get-ServerQuit([string]$Dir, [string]$Tag) {
    $log = Join-Path $Dir "appdata-$Tag/standin.log"
    if (-not (Test-Path -LiteralPath $log)) { return 'no log' }
    $quits = @(Get-Content -LiteralPath $log | Where-Object { $_ -like '*"ev":"quit"*' })
    if ($quits.Count -eq 0) { return 'none: the process was ended from outside before it could quit' }
    return ($quits[-1] | ConvertFrom-Json).why
}

# The most keystrokes the profile's localStorage files hold on disk. Chromium keeps
# localStorage in a LevelDB folder in the profile, and the page's draft names its count as
# "n":<count>. The files are opened with full sharing, so Chromium can go on writing them.
function Get-CommittedKeystrokes([string]$ProfileDir) {
    $folder = Join-Path $ProfileDir 'Local Storage/leveldb'
    if (-not (Test-Path -LiteralPath $folder)) { return 0 }
    $best = 0
    foreach ($file in @(Get-ChildItem -LiteralPath $folder -File -ErrorAction SilentlyContinue)) {
        if (@('.log', '.ldb') -notcontains $file.Extension) { continue }
        $bytes = $null
        $read = 0
        try {
            $stream = [IO.File]::Open($file.FullName, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]'ReadWrite, Delete')
            try {
                $bytes = New-Object byte[] ([int]$stream.Length)
                while ($read -lt $bytes.Length) {
                    $got = $stream.Read($bytes, $read, $bytes.Length - $read)
                    if ($got -le 0) { break }
                    $read += $got
                }
            } finally {
                $stream.Dispose()
            }
        } catch {
            continue
        }
        $text = [Text.Encoding]::GetEncoding(28591).GetString($bytes, 0, $read)
        foreach ($match in [regex]::Matches($text, '"n":(\d+)')) {
            $value = [int]$match.Groups[1].Value
            if ($value -gt $best) { $best = $value }
        }
    }
    return $best
}

function Fmt([object]$Value) {
    if ($null -eq $Value) { return '-' }
    return Format-Invariant '{0:0.0}' ([double]$Value)
}

function Verdict([object]$Probe) {
    # An informational probe says what it found; it is never a gap in AD11.
    if ($Probe.finding) { return "$($Probe.finding)" }
    if ($null -eq $Probe.pass) { return 'skipped' }
    if ($Probe.pass) { return 'passes' }
    return '**fails**'
}

$failures = @()
$retries = @()
$restarts = @()
$closes = @()
$ends = @()
$kills = @()

# Each case gets a second attempt in a fresh folder if the first one fails; the report lists
# every first attempt that failed, so a retry never hides a problem.
function Invoke-Case([string]$Name, [string]$Dir, [scriptblock]$Body) {
    for ($attempt = 1; $attempt -le 2; $attempt++) {
        $where = if ($attempt -eq 1) { $Dir } else { "$Dir-retry" }
        try {
            return (& $Body $where)
        } catch {
            $message = $_.Exception.Message
            Say "FAILED ($Name, attempt $attempt): $message"
            if ($attempt -eq 1) {
                $script:retries += "${Name}: the first attempt failed ($message); a second attempt ran"
            } else {
                $script:failures += "${Name}: $message"
            }
        } finally {
            Stop-Leftovers $script:caseProcesses
            $script:caseProcesses = @()
        }
    }
    return $null
}

$script:caseProcesses = @()
function Track([int[]]$Ids) { $script:caseProcesses += $Ids }

# ------------------------------------------------------------------ run: probes and round trips

$modes = @('filter', 'protocol', 'filter-all')
# Each mode runs once per pass; later passes alternate the order, so a drift over the run (the
# laptop warming up, a background task) does not fall on one mode only.
$runs = @()
if (Want 'run') {
    for ($pass = 1; $pass -le $Passes; $pass++) {
        $order = if ($pass % 2 -eq 1) { $modes } else { @($modes[($modes.Count - 1)..0]) }
        foreach ($mode in $order) {
            Say "Run, pass $pass of $Passes, $mode mode: probes, then $Clicks clicks alone and $Clicks while pictures load"
            $result = Invoke-Case "run, $mode mode, pass $pass" (Join-Path $Out "run-$mode-$pass") {
                param($dir)
                $shell = Start-Shell $dir $mode 'run' @{}
                Track @($shell.Id)
                if (-not (Wait-ShellEvent $dir "$mode-run" 'page-loaded' 1 120000)) { throw 'the page did not load in 2 minutes' }
                if (-not $shell.WaitForExit(20 * 60 * 1000)) { throw "the $mode run took more than 20 minutes" }
                $result = Read-Json (Join-Path $dir "results-$mode-run.json")
                if ($null -eq $result) { throw "the $mode run wrote no results; see $dir" }
                if ($result.error) { throw "the page failed: $($result.error)" }
                $result
            }
            if ($result) { $runs += [pscustomobject]@{ Mode = $mode; Pass = $pass; Result = $result } }
        }
    }
}

function Get-Runs([string]$Mode) {
    return @($runs | Where-Object { $_.Mode -eq $Mode } | Sort-Object Pass)
}

# ------------------------------------------------------------------ restart: end the server

if (Want 'restart') {
    foreach ($mode in @('filter', 'protocol')) {
        Say "Restart, $mode mode: end the server; the shell starts it again"
        $row = Invoke-Case "restart, $mode mode" (Join-Path $Out "restart-$mode") {
            param($dir)
            $tag = "$mode-home"
            $shell = Start-Shell $dir $mode 'home' @{}
            Track @($shell.Id)
            $first = Wait-ShellEvent $dir $tag 'home-loaded' 1 60000
            if (-not $first) { throw 'Home did not load' }
            $status = Read-Json (Join-Path $dir "status-$tag.json")
            Track @($status.serverPid)
            $killed = Get-NowMs
            Stop-Process -Id $status.serverPid -Force
            $exited = Wait-ShellEvent $dir $tag 'server-exited' 1 10000
            $again = Wait-ShellEvent $dir $tag 'server-started' 2 30000
            $home2 = Wait-ShellEvent $dir $tag 'home-loaded' 2 30000
            if ($again) { Track @([int]$again.pid) }
            Send-Control $dir $tag 'quit'
            [void]$shell.WaitForExit(15000)
            [pscustomobject]@{
                Mode      = $mode
                Seen      = if ($exited) { $exited.t - $killed } else { $null }
                Address   = if ($again) { $again.t - $killed } else { $null }
                Home      = if ($home2) { $home2.t - $killed } else { $null }
                NewServer = [bool]($home2 -and $home2.instance -ne $first.instance)
                NewPort   = [bool]($home2 -and $home2.port -ne $first.port)
            }
        }
        if ($row) { $restarts += $row }
    }
}

# ------------------------------------------------------------------ close: a normal close

if (Want 'close') {
    Say 'Close: close the window as a person would'
    $row = Invoke-Case 'close' (Join-Path $Out 'close') {
        param($dir)
        $tag = 'filter-home'
        $shell = Start-Shell $dir 'filter' 'home' @{}
        Track @($shell.Id)
        if (-not (Wait-ShellEvent $dir $tag 'home-loaded' 1 60000)) { throw 'Home did not load' }
        $status = Read-Json (Join-Path $dir "status-$tag.json")
        Track @($status.serverPid)
        $how = 'the shell was asked to close its window'
        $closedAt = Get-NowMs
        $sent = $false
        if ($onWindows) {
            $shell.Refresh()
            if ($shell.MainWindowHandle -ne [IntPtr]::Zero) {
                $sent = $shell.CloseMainWindow()
                if ($sent) { $how = 'WM_CLOSE to the window, as its close button sends' }
            }
        }
        if (-not $sent) { Send-Control $dir $tag 'close' }
        $serverMs = Wait-Gone $status.serverPid $closedAt 10000
        $shellExited = $shell.WaitForExit(10000)
        [pscustomobject]@{
            How      = $how
            ServerMs = $serverMs
            Why      = Get-ServerQuit $dir $tag
            Shell    = if ($shellExited) { 'exited' } else { 'was still running after 10 s' }
        }
    }
    if ($row) { $closes += $row }
}

# ------------------------------------------------------------------ endshell: end the shell's process

if (Want 'endshell') {
    foreach ($spawn in @('job', 'detached')) {
        $how = if ($spawn -eq 'job') { 'plainly' } else { 'detached' }
        Say "End the shell (server started $how)"
        $row = Invoke-Case "endshell, server started $how" (Join-Path $Out "endshell-$spawn") {
            param($dir)
            $extra = @{ spawn = $spawn }
            $tag = Get-Tag 'filter' 'home' $extra
            $shell = Start-Shell $dir 'filter' 'home' $extra
            Track @($shell.Id)
            if (-not (Wait-ShellEvent $dir $tag 'home-loaded' 1 60000)) { throw 'Home did not load' }
            $status = Read-Json (Join-Path $dir "status-$tag.json")
            Track @($status.serverPid)
            $children = @(Get-ChildProcesses $shell.Id)
            Track @($children | ForEach-Object { $_.Id })
            $killed = Get-NowMs
            Stop-Process -Id $shell.Id -Force
            $serverMs = Wait-Gone $status.serverPid $killed 10000
            Start-Sleep -Seconds 5
            $left = @($children | Where-Object { $_.Id -ne $status.serverPid -and (Test-Alive $_.Id) })
            [pscustomobject]@{
                Spawn    = $spawn
                ServerMs = $serverMs
                Why      = Get-ServerQuit $dir $tag
                Children = ($children | ForEach-Object { $_.Name }) -join ', '
                Left     = if ($left.Count) { ($left | ForEach-Object { $_.Name }) -join ', ' } else { 'none' }
            }
        }
        if ($row) { $ends += $row }
    }
}

# ------------------------------------------------------------------ hardkill: typed text after a kill

if (Want 'hardkill') {
    # Each mode twice: a hard kill, and a normal quit as the control that shows the inspection
    # can read what was kept.
    foreach ($mode in @('filter', 'protocol')) {
        foreach ($end in @('kill', 'quit')) {
            $what = if ($end -eq 'kill') { 'Hard kill' } else { 'Normal quit (the control)' }
            Say "$what while typing, $mode mode"
            $folder = if ($end -eq 'kill') { "hardkill-$mode" } else { "typequit-$mode" }
            $row = Invoke-Case "hardkill, $mode mode, $end" (Join-Path $Out $folder) {
                param($dir)
                $tag = "$mode-type"
                $shell = Start-Shell $dir $mode 'type' @{}
                Track @($shell.Id)
                $typing = Wait-ShellEvent $dir $tag 'typing-started' 1 60000
                if (-not $typing) { throw 'typing did not start' }
                $status = Read-Json (Join-Path $dir "status-$tag.json")
                Track @($status.serverPid)
                # Chromium writes localStorage to the profile some seconds after the page sets
                # it. End the shell only after that first write and -AfterCommit seconds more,
                # so what localStorage keeps is not decided by when the script acts.
                $profileDir = Join-Path $dir "profile-$mode"
                $commitDeadline = (Get-NowMs) + 60000
                $committedMs = $null
                while ((Get-NowMs) -lt $commitDeadline) {
                    if ((Get-CommittedKeystrokes $profileDir) -gt 0) { $committedMs = (Get-NowMs) - $typing.t0; break }
                    Start-Sleep -Milliseconds 250
                }
                Start-Sleep -Seconds $AfterCommit
                $onDisk = Get-CommittedKeystrokes $profileDir
                $acted = Get-NowMs
                if ($end -eq 'kill') {
                    Stop-Process -Id $shell.Id -Force
                } else {
                    Send-Control $dir $tag 'quit'
                    if (-not $shell.WaitForExit(15000)) { throw 'the shell did not quit' }
                }
                $serverMs = Wait-Gone $status.serverPid $acted 10000
                Start-Sleep -Seconds 2
                Stop-Leftovers @($status.serverPid)
                $made = [math]::Floor(($acted - $typing.t0) / $typing.every)
                $typedLog = Join-Path $dir "appdata-$tag/typed.log"
                $serverSaw = 0
                if (Test-Path -LiteralPath $typedLog) {
                    $last = Get-Content -LiteralPath $typedLog | Where-Object { $_ } | Select-Object -Last 1
                    if ($last) { $serverSaw = ($last | ConvertFrom-Json).n }
                }
                Say "  inspecting what the $mode profile kept"
                $inspect = Start-Shell $dir $mode 'inspect' @{ port = $status.port }
                Track @($inspect.Id)
                if (-not $inspect.WaitForExit(60000)) { throw 'the inspection did not finish' }
                $seen = Read-Json (Join-Path $dir "results-$mode-inspect.json")
                if ($null -eq $seen) { throw 'the inspection wrote no results' }
                [pscustomobject]@{
                    Mode       = $mode
                    End        = if ($end -eq 'kill') { 'hard kill' } else { 'normal quit (control)' }
                    Made       = $made
                    Every      = $typing.every
                    Committed  = $committedMs
                    OnDisk     = $onDisk
                    ServerSaw  = $serverSaw
                    Local      = $seen.localStorage.n
                    LocalError = $seen.localStorage.error
                    Mirror     = $seen.mirror.n
                    Torn       = $seen.shell.mirrorAtLaunch.torn
                    Lines      = $seen.shell.mirrorAtLaunch.lines
                    ServerMs   = $serverMs
                    Why        = Get-ServerQuit $dir $tag
                }
            }
            if ($row) { $kills += $row }
        }
    }
}

# ------------------------------------------------------------------ the report

$finished = (Get-Date).ToUniversalTime()
# The results, events and screenshots go beside the report; profiles, logs and downloads stay
# in $Out.
$raw = Join-Path (Split-Path -Parent $Report) ([IO.Path]::GetFileNameWithoutExtension($Report))
New-Item -ItemType Directory -Force -Path $raw | Out-Null
foreach ($file in (Get-ChildItem -LiteralPath $Out -Recurse -File)) {
    if ($file.Name -match '^(results-.*\.json|events-.*\.jsonl|graphite-.*\.png)$') {
        $case = Split-Path -Leaf $file.DirectoryName
        Copy-Item -LiteralPath $file.FullName -Destination (Join-Path $raw "$case-$($file.Name)") -Force
    }
}
# The shell never writes a token, and this checks it: a fragment token in a copied file is
# taken out and named in the report.
$leaks = @()
foreach ($file in (Get-ChildItem -LiteralPath $raw -File | Where-Object { $_.Extension -ne '.png' })) {
    $text = [IO.File]::ReadAllText($file.FullName)
    if ($text -match '#t=[A-Za-z0-9_-]{20,}') {
        Write-Utf8File $file.FullName ($text -replace '#t=[A-Za-z0-9_-]{20,}', '#t=<token>')
        $leaks += $file.Name
    }
}
$rawRel = $raw.Replace($repo, '.').Replace('\', '/')
$anyRun = $null
foreach ($run in $runs) { if ($run.Result.shell) { $anyRun = $run.Result; break } }
$versions = if ($anyRun) { $anyRun.shell.versions } else { $null }
$md = New-Object System.Collections.Generic.List[string]
function Add([string]$Line) { $md.Add($Line) }

$platform = if ($onWindows) { 'Windows' } else { 'Linux' }
Add "# Electronite spike G0 on $platform ($stamp)"
Add ''
Add 'This file was written by `spikes/electronite/run-spike.ps1`. The spike is throwaway and never'
Add 'shipped (docs/APP-DESIGN.md section 10, unit G0). **The server is a stand-in**: `tc4-standin`'
Add '(`spikes/electronite/src`), a small axum server on 127.0.0.1 with the per-launch token, the'
Add 'Host, Origin and X-Tc-Api checks of section 5.2, an address line on stdout and a quit on a closed stdin,'
Add 'because the real `tc4 serve` (unit A2) is being built at the same time. Its loopback cost is not'
Add "tc4 serve's: unit A2 measures that. What this file measures is the shell, the browser engine and"
Add 'the operating system around a server of the same kind.'
Add ''
Add "- Commit: $commit"
Add ("- Run: {0} to {1} UTC; ``-Clicks $Clicks -Warmup $Warmup``" -f $started.ToString('yyyy-MM-dd HH:mm'), $finished.ToString('HH:mm'))
if ($versions) {
    Add "- Electronite $($versions.electron), Chromium $($versions.chrome), Node $($versions.node), V8 $($versions.v8)"
} else {
    Add '- Electronite and Chromium versions: unavailable (no run case finished)'
}
Add ('- Raw results: `{0}` (results, events and screenshots; the full run is in `{1}`)' -f $rawRel, ($Out.Replace($repo, '.').Replace('\', '/')))
Add ''
Add '## Machine'
Add ''
foreach ($line in (Format-MachineTable $machine)) { Add $line }
Add ''

if ($failures.Count -gt 0) {
    Add '## Cases that did not finish'
    Add ''
    foreach ($f in $failures) { Add "- $f" }
    Add ''
}
if ($leaks.Count -gt 0) {
    Add '## A token was found and removed'
    Add ''
    Add "These copied files held a launch token, now replaced by ``<token>``: $($leaks -join ', '). This is a bug in the spike."
    Add ''
}
if ($retries.Count -gt 0) {
    Add '## First attempts that failed'
    Add ''
    foreach ($r in $retries) { Add "- $r" }
    Add ''
}

if ($runs.Count -gt 0) {
    Add '## 1. Request types (AD11, Q-1)'
    Add ''
    Add 'A probe passes when the page saw the load succeed and the server saw the request with the'
    Add 'token. `filter` is AD11 (onBeforeSendHeaders on the paths outside `/api/`), `protocol` is'
    Add 'the Q-1 prototype (a privileged `tc4://` scheme proxied by `protocol.handle`), `filter-all`'
    Add 'is a filter over every path. A cell that differs between passes shows each pass in turn.'
    Add 'Rows marked informational are not request types the app relies on, and never count as a'
    Add 'gap in AD11.'
    Add ''
    Add '`api-main-thread` times the page''s own `/api/` fetch with the shell''s main thread idle, then'
    Add 'while that thread is blocked 200 ms of every 250 ms. Electron passes every request of the'
    Add 'window through a proxy on that thread, whatever the filter''s URL list says; the list only'
    Add 'spares the filter''s JS listener (`api-listener`). "Waits for the main process" means the'
    Add 'busy p95 rose by more than 50 ms.'
    Add ''
    $present = @($modes | Where-Object { (Get-Runs $_).Count -gt 0 })
    Add ('| Request | ' + ($present -join ' | ') + ' |')
    Add ('|---|' + (($present | ForEach-Object { '---' }) -join '|') + '|')
    $first = (Get-Runs $present[0])[0].Result
    foreach ($probe0 in $first.probes) {
        $name = $probe0.name
        $cells = foreach ($mode in $present) {
            $seen = @(Get-Runs $mode | ForEach-Object {
                    $probe = $_.Result.probes | Where-Object { $_.name -eq $name } | Select-Object -First 1
                    if ($probe) { Verdict $probe } else { '-' }
                })
            $distinct = @($seen | Select-Object -Unique)
            if ($distinct.Count -le 1) { $distinct[0] } else { $seen -join ' / ' }
        }
        Add ("| $name ($($probe0.what)) | " + ($cells -join ' | ') + ' |')
    }
    Add ''
    Add 'What each probe saw:'
    Add ''
    foreach ($run in $runs) {
        foreach ($probe in $run.Result.probes) {
            Add ("- $($run.Mode), pass $($run.Pass), $($probe.name): " + ("$($probe.detail)" -replace '\|', '/'))
        }
    }
    Add ''
    Add '## 2. The aligner round trip (AD18, Q-13)'
    Add ''
    Add 'One click: the page posts to `/api/v1/align/click` with its own fetch, gets about 20 KB of'
    Add 'JSON, parses it and rebuilds the word bank and the cards. "Reply" ends when the JSON is'
    Add 'parsed, "rendered" when the DOM is rebuilt, "frame" at the start of the next animation frame,'
    Add 'and "painted" when the main thread has finished that frame''s style, layout and paint (a'
    Add 'message posted from the frame''s requestAnimationFrame callback, which Chromium handles only'
    Add 'after them; the compositor shows the frame at the next display refresh). Painted is the'
    Add 'figure for Q-13 and Q-1. Times in ms, nearest-rank, after the warm-up clicks. While loading:'
    Add "a story's 15 OBS-sized pictures (640x360, about 70 KB) every 500 ms, the help page every"
    Add 'second, and the event stream open. The budget is 30 ms (PERFORMANCE.md, Aligning).'
    Add ''
    Add '| Mode | Pass | While | Clicks | Reply median | Reply p95 | Rendered p95 | Frame p95 | Painted median | Painted p95 | Frames missed | Loaded meanwhile |'
    Add '|---|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|---|'
    foreach ($mode in $present) {
        foreach ($run in (Get-Runs $mode)) {
            foreach ($rt in $run.Result.roundTrip) {
                $loaded = '-'
                if ($rt.load) { $loaded = "$($rt.load.pictures) pictures, $($rt.load.helpPages) help pages, $($rt.load.events) events" }
                Add ("| $mode | $($run.Pass) | $($rt.label) | $($rt.clicks) | $(Fmt $rt.reply.median) | $(Fmt $rt.reply.p95) | $(Fmt $rt.rendered.p95) | $(Fmt $rt.frame.p95) | $(Fmt $rt.painted.median) | $(Fmt $rt.painted.p95) | $($rt.frameTimeouts) | $loaded |")
            }
        }
    }
    Add ''
    Add 'The painted p95 of each pass, and its spread (the highest minus the lowest):'
    Add ''
    Add '| Mode | While | Painted p95 by pass | Lowest | Highest | Spread |'
    Add '|---|---|---|---:|---:|---:|'
    $ranges = @{}
    foreach ($mode in $present) {
        foreach ($label in @('alone', 'loading')) {
            $values = @(Get-Runs $mode | ForEach-Object {
                    $rt = $_.Result.roundTrip | Where-Object { $_.label -eq $label } | Select-Object -First 1
                    if ($rt -and $null -ne $rt.painted.p95) { [double]$rt.painted.p95 }
                })
            if ($values.Count -eq 0) { continue }
            $low = ($values | Measure-Object -Minimum).Minimum
            $high = ($values | Measure-Object -Maximum).Maximum
            $ranges["$mode/$label"] = @($low, $high)
            Add ("| $mode | $label | " + (($values | ForEach-Object { Fmt $_ }) -join ', ') + " | $(Fmt $low) | $(Fmt $high) | $(Fmt ($high - $low)) |")
        }
    }
    Add ''
    foreach ($label in @('alone', 'loading')) {
        $f = $ranges["filter/$label"]
        $p = $ranges["protocol/$label"]
        if (-not $f -or -not $p) { continue }
        if ($Passes -lt 2) {
            $line = 'one pass only: no spread, so this run alone does not decide Q-1'
        } elseif ($p[1] -lt $f[0]) {
            $line = "protocol lower in every pass, by $(Fmt ($f[0] - $p[1])) ms or more"
        } elseif ($f[1] -lt $p[0]) {
            $line = "filter lower in every pass, by $(Fmt ($p[0] - $f[1])) ms or more"
        } else {
            $line = 'the passes overlap: the difference is inside the spread and does not decide Q-1'
        }
        Add "- Q-1, $label (painted p95, protocol against filter): $line."
    }
    Add ''
}

if ($restarts.Count -gt 0 -or $closes.Count -gt 0 -or $ends.Count -gt 0) {
    Add '## 3. Supervision (AD39)'
    Add ''
    Add 'Times in ms from the moment the script acted.'
    Add ''
    Add '| Case | Result |'
    Add '|---|---|'
    foreach ($r in $restarts) {
        Add ("| End the server process, $($r.Mode) mode | the shell saw the exit after $(Fmt $r.Seen), had a new server address after $(Fmt $r.Address), and Home was loaded again after $(Fmt $r.Home); new server: $($r.NewServer), new port: $($r.NewPort) |")
    }
    foreach ($c in $closes) {
        Add ("| Close the window ($($c.How)) | the server exited after $(Fmt $c.ServerMs) (its quit: $($c.Why)); the shell $($c.Shell) |")
    }
    foreach ($e in $ends) {
        $how = if ($e.Spawn -eq 'job') { 'server started plainly (Node''s default spawn)' } else { 'server started with detached: true' }
        $exit = if ($null -eq $e.ServerMs) { '**the server was still running after 10 s**' } else { "the server exited after $(Fmt $e.ServerMs)" }
        Add ("| End the shell's process ($how) | $exit; its quit: $($e.Why). Child processes before: $($e.Children). Left after 5 s: $($e.Left) |")
    }
    Add ''
}

if ($kills.Count -gt 0) {
    Add '## 4. A hard kill while typing (AD21)'
    Add ''
    Add 'The page types one letter every 50 ms. Each keystroke is written to `localStorage` at once,'
    Add "sent to the shell, which appends it to its mirror file (synced at most once a second), and"
    Add 'reported to the server. Chromium writes `localStorage` to the profile some seconds after the'
    Add 'page sets it. The script waits until the profile''s files on disk hold the draft (the "first'
    Add ('write to disk" column), lets the page type ' + $AfterCommit + ' s more, then ends the shell''s process (or, as the')
    Add 'control, asks the shell to quit normally), opens the same profile again (same origin) and'
    Add 'reads what was kept. Counts are keystrokes; "made" counts them from the clock up to the moment'
    Add 'the script acted, so a normal quit can keep a few more. "Lost" is the typing time missing at'
    Add 'the end: (made - kept) x 50 ms.'
    Add ''
    Add '| Mode | How the shell ended | Made (from the clock) | First write to disk | On disk just before | The server saw | localStorage kept | localStorage lost | The mirror kept | The mirror lost | Mirror lines (torn) | The server |'
    Add '|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|---|'
    foreach ($k in $kills) {
        $every = if ($k.Every) { [double]$k.Every } else { 50.0 }
        $local = if ($k.LocalError) { "error: $($k.LocalError)" } else { "$($k.Local)" }
        $localLost = if ($k.LocalError) { '-' } else { "$(Fmt ([math]::Max(0, $k.Made - [double]$k.Local) * $every / 1000)) s" }
        $mirrorLost = "$(Fmt ([math]::Max(0, $k.Made - [double]$k.Mirror) * $every / 1000)) s"
        $firstWrite = if ($null -eq $k.Committed) { '**none in 60 s**' } else { "$(Fmt ($k.Committed / 1000)) s after typing began" }
        $server = if ($null -eq $k.ServerMs) { 'still running after 10 s' } else { "exited after $(Fmt $k.ServerMs) (quit: $($k.Why))" }
        Add ("| $($k.Mode) | $($k.End) | $($k.Made) | $firstWrite | $($k.OnDisk) | $($k.ServerSaw) | $local | $localLost | $($k.Mirror) | $mirrorLost | $($k.Lines) ($($k.Torn)) | $server |")
    }
    Add ''
}

if ($anyRun -and $anyRun.graphite) {
    Add '## 5. Graphite (informational)'
    Add ''
    Add 'Awami Nastaliq has Graphite tables and no OpenType substitutions: with Graphite its letters'
    Add 'join and cascade; without it they stand alone and the line is about twice as wide. In'
    Add 'Chromium 141 without Graphite (Playwright, `spikes/electronite/test`) the test text measured'
    Add '639.8 px wide with an ascent of 46 px; Electronite 37.1.0 on Linux measured 302.9 px and'
    Add '61 px. "Joined" below means narrower than 480 px. This is a smoke test, not the P8 probe.'
    Add ''
    Add '| Mode | Font loaded | Width | Height | Canvas width | Ascent | Descent | Joined | Screenshot |'
    Add '|---|---|---:|---:|---:|---:|---:|---|---|'
    foreach ($mode in $modes) {
        $modeRuns = @(Get-Runs $mode)
        if ($modeRuns.Count -eq 0) { continue }
        $g = $modeRuns[0].Result.graphite
        $joined = if ($g.fontLoaded -and [double]$g.boxWidth -lt 480) { 'yes' } else { '**no**' }
        Add ("| $mode | $($g.fontLoaded) | $(Fmt $g.boxWidth) | $(Fmt $g.boxHeight) | $(Fmt $g.canvasWidth) | $(Fmt $g.ascent) | $(Fmt $g.descent) | $joined | ``$($g.screenshot)`` |")
    }
    Add ''
}

Add '## What these results decide'
Add ''
Add '- **AD11 and capability URLs.** A request type that fails under `filter` above is one that'
Add '  the header filter misses; section 5.2 then moves `/media`, `/download` and `/print` to capability'
Add '  URLs. A failure under `filter` that also fails under `filter-all` is not the filter''s list.'
Add '  Informational rows (EventSource, `api-main-thread`) are never such a failure.'
Add '- **The main process.** `api-main-thread` shows whether the page''s `/api/` requests wait for'
Add '  the shell''s main thread. If they do, the shell must keep synchronous work off that thread'
Add '  (the mirror''s appends and fsync in G1), or every aligner click waits for it.'
Add '- **Q-1, the custom scheme.** The design''s rule: take `protocol.handle` if it adds less to'
Add '  the aligner''s round trip than the filter, otherwise the filter. Compare the painted p95 of'
Add '  `protocol` and `filter`, alone and while loading, across the passes, and the probe rows. A'
Add '  difference inside the spread between passes does not decide Q-1.'
Add '- **Q-13, the aligner fallback.** If the painted p95 of `filter` misses 30 ms in any pass on'
Add '  the reference laptop, Q-13 goes to the owner with these numbers.'
Add '- **AD39 and G1.** The restart rows show whether the shell restarts the server and reloads'
Add '  Home. The "end the shell" rows show whether the server quits by itself when the shell dies,'
Add '  or is ended with it, which decides how G1 starts `tc4`.'
Add '- **AD21.** The hard-kill rows show how many keystrokes, and how many seconds of typing,'
Add '  `localStorage` and the shell''s mirror keep when the whole app dies after Chromium has'
Add '  written `localStorage` once.'
Add ''

Write-Utf8File $Report (($md -join "`n") + "`n")
Say "Wrote $Report"
if ($failures.Count -gt 0) {
    Say "$($failures.Count) case(s) did not finish; they are listed in the report."
    exit 1
}
