# Task 5 step 5: the G0 spike's Electronite shell with its page (filter mode, the Home page, idle),
# as part 1 of .audit/tasks/g0-windows.md starts it; every electron process, and the stand-in
# server apart (it is not tc4 serve).
$ErrorActionPreference = 'Stop'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
# The spike next to this folder; build its stand-in server first (g0-windows.md part 1).
$spike = (Resolve-Path (Join-Path $PSScriptRoot '..\electronite')).Path
$electron = 'C:\tc4-g0\electronite\electron.exe'
$server = "$spike\target\release\tc4-standin.exe"
$out = 'C:\tc4-g0\memory\electronite'
if (Get-Process electron -ErrorAction SilentlyContinue) { throw 'An electron process is already running.' }
New-Item -ItemType Directory -Force $out | Out-Null
$argv = @("`"$spike\shell\main.cjs`"", '--spike-mode=filter', '--spike-case=home', "--spike-out=`"$out`"", "--spike-server=`"$server`"")
$p = Start-Process -FilePath $electron -ArgumentList $argv -PassThru -RedirectStandardOutput "$out\shell.out" -RedirectStandardError "$out\shell.err"
Start-Sleep -Seconds 15
$events = Get-Content "$out\events-filter-home.jsonl" -ErrorAction SilentlyContinue
"shell pid $($p.Id); events: " + (($events | ForEach-Object { ($_ | ConvertFrom-Json).ev }) -join ', ')
$v = (Get-Item $electron).VersionInfo
"electron.exe $($v.ProductName) $($v.ProductVersion)"
& (Join-Path $here 'measure.ps1') -Label "Electronite $($v.ProductVersion) shell with the G0 spike's Home page, idle (filter mode); every electron process" -Instance 'electron*' -Match 'electron'
& (Join-Path $here 'measure.ps1') -Label 'The G0 stand-in server (tc4-standin, not tc4 serve), beside that window' -Instance 'tc4-standin*' -Match 'tc4-standin' -WaitFirst 0 -Gap 5
# A normal close, through the shell's control file.
Set-Content -Path "$out\control-filter-home.txt" -Value 'close'
Start-Sleep -Seconds 5
"electron left running: $(@(Get-Process electron -ErrorAction SilentlyContinue).Count); stand-in left running: $(@(Get-Process tc4-standin -ErrorAction SilentlyContinue).Count)"
