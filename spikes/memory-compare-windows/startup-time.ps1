# Startup timing, the same way for every program (Word, Edge, tC4 4.0, and later the rebuild):
#
#   t0       just before the launch, which goes through ShellExecute as a double-click does
#            (Start-Process on the document, or on the program's Start-menu shortcut).
#   window   the first visible top-level window of the program's processes, at least 200 x 150.
#   painted  that window shows a picture rather than one flat colour (Test-Painted in
#            startup-lib.ps1): a blank white or grey first frame fails this.
#   usable   the program's own test (-Usable) holds, polled at the same rate; then the run stops.
#
# Polling runs every 15 ms on one thread, so each moment is late by at most about 15 ms plus one
# screen capture (a few ms). Windows are captured from the screen, so none may be covered.
#
# usage: startup-time.ps1 -Name WINWORD -Launch C:\path\Psalms.docx -Usable { param($win) ... }
#        [-Label text] [-AllowRunning] [-Timeout 120] [-Out startup.jsonl]
param(
  [Parameter(Mandatory)] [string] $Name,          # process name without .exe
  [Parameter(Mandatory)] [string] $Launch,
  [string[]] $Arguments = @(),
  [scriptblock] $Usable = { param($win) $true },
  [string] $Label = $Name,
  [int] $Timeout = 120,
  # Edge keeps background processes (startup boost); then only a visible window must be absent.
  [switch] $AllowRunning,
  [string] $Out = 'C:\tc4-g0\memory\startup.jsonl'
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'startup-lib.ps1')
if ($AllowRunning) {
  if (Get-Window $Name) { throw "$Name already has a window open; close it first." }
} elseif (Get-Process -Name $Name -ErrorAction SilentlyContinue) { throw "$Name is already running; a start must begin with it closed." }
$before = Get-Conditions
$sw = [Diagnostics.Stopwatch]::StartNew()
if ($Arguments.Count) { Start-Process -FilePath $Launch -ArgumentList $Arguments } else { Start-Process -FilePath $Launch }
$t = [ordered]@{ window_ms = $null; painted_ms = $null; usable_ms = $null }
while ($sw.Elapsed.TotalSeconds -lt $Timeout) {
  $win = Get-Window $Name
  if ($win) {
    if ($null -eq $t.window_ms) { $t.window_ms = $sw.ElapsedMilliseconds }
    # A window that is still being laid out can refuse a capture or a focus query: not yet.
    try {
      if ($null -eq $t.painted_ms -and (Test-Painted $win.Rect)) { $t.painted_ms = $sw.ElapsedMilliseconds }
      if ($null -ne $t.painted_ms -and (& $Usable $win)) { $t.usable_ms = $sw.ElapsedMilliseconds; break }
    } catch { }
  }
  Start-Sleep -Milliseconds 15
}
$row = [pscustomobject]@{
  label = $Label; at = (Get-Date).ToString('s')
  window_ms = $t.window_ms; painted_ms = $t.painted_ms; usable_ms = $t.usable_ms; timed_out = ($null -eq $t.usable_ms)
  available_mb_before = $before.available_mb_before; builds_running = $before.builds_running; minutes_since_boot = $before.minutes_since_boot
}
($row | ConvertTo-Json -Compress) | Out-File -Append -Encoding utf8 $Out
$row | ConvertTo-Json
