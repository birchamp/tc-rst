# tC4 4.0's start, step by step, with startup-lib.ps1's tests (see startup-time.ps1 for the
# method): launch through the Start-menu shortcut, the window, painted, Home usable (the blue
# "+ Add a project" button drawn), then a click on the Psalms tile of the first project, the
# project open (its top bar's "Understand" tab drawn active), then a click on "Translate", and
# Translate open (that tab drawn active and the page below it painted).
#
# Positions are fractions of tC4's maximized window on this 1920 x 1080 laptop at 150 % scale.
# -NoLaunch times only the clicks, from a Home that is already showing.
param(
  [string] $Label = 'tC4 4.0.0-rc.2',
  [switch] $NoLaunch,
  [int] $Timeout = 120,
  [string] $Out = 'C:\tc4-g0\memory\startup.jsonl',
  [string] $Shortcut = "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\translationCore4.lnk"
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'startup-lib.ps1')
$blue = '#31ADE3'          # buttons and the active tab
$hovered = '#2599CC'       # the active tab under the mouse, as it is right after a click
# Home: the blue "+ Add a project" button (and "Resume", once a project was opened) drawn in
# the right-hand column. A project's Notes panel there has only small blue badges.
$addButton = { param($r) Test-Colour $r 0.74 0.10 0.92 0.50 $blue 0.05 }
# Once a project was opened, Home shows a navy "Resume" banner (#014263) at the top, and the
# project's Psalms tile moves down.
$hasResume = { param($r) Test-Colour $r 0.55 0.24 0.70 0.33 '#014263' 0.8 }
# The project list is drawn: the project card's navy avatar square, where it sits with or
# without the banner. The button can be drawn before the list arrives, so Home waits for both.
$cardHigh = { param($r) Test-Colour $r 0.146 0.355 0.170 0.395 '#014263' 0.6 }
$cardLow = { param($r) Test-Colour $r 0.146 0.545 0.170 0.585 '#014263' 0.6 }
$homeReady = { param($r) (& $addButton $r) -and (((& $hasResume $r) -and (& $cardLow $r)) -or (-not (& $hasResume $r) -and (& $cardHigh $r))) }
# One of the three tabs (Understand, Translate, Check) is drawn active: tC4 may open the tab used last.
$projectReady = { param($r) (Test-Colour $r 0.33 0.03 0.56 0.07 $blue 0.10) -or (Test-Colour $r 0.33 0.03 0.56 0.07 $hovered 0.10) }
$translateReady = { param($r) (Test-Colour $r 0.43 0.03 0.49 0.07 $blue 0.35) -or (Test-Colour $r 0.43 0.03 0.49 0.07 $hovered 0.35) }
# Translate is usable once Psalm 1's verses are drawn: text (#231F20 on #F7FAFC) in the source
# column.
$textReady = { param($r) Test-Colour $r 0.20 0.32 0.45 0.62 '#231F20' 0.008 60 }
function Wait-For($check, $sw) {
  while ($sw.Elapsed.TotalSeconds -lt $Timeout) {
    $win = Get-Window electron
    # A window that is still being laid out can refuse a capture: not yet.
    try { if ($win -and (& $check $win.Rect)) { return $sw.ElapsedMilliseconds } } catch { }
    Start-Sleep -Milliseconds 15
  }
  $null
}
$t = [ordered]@{ window_ms = $null; painted_ms = $null; home_ms = $null; project_ms = $null; translate_ms = $null; text_ms = $null }
$clickHome = $null; $clickTranslate = $null; $resume = $null; $launch = $null
$before = Get-Conditions
$sw = [Diagnostics.Stopwatch]::StartNew()
if (-not $NoLaunch) {
  if (Get-Process electron -ErrorAction SilentlyContinue) { throw 'tC4 is already running; a start must begin with it closed.' }
  try {
    Start-Process -FilePath $Shortcut -ErrorAction Stop
    $launch = 'shortcut'
  } catch {
    # The first start after a reboot once failed here with "The system cannot find the file
    # specified", for a shortcut that exists and had started tC4 about twenty times that morning.
    # Record what was on disk, then start what the shortcut starts, so the run still measures
    # tC4 and not the shell's lookup of the shortcut. The clock restarts at that launch, and the
    # row says how tC4 was started.
    $why = $_.Exception.Message
    $link = (New-Object -ComObject WScript.Shell).CreateShortcut($Shortcut)
    $seen = [ordered]@{
      at = (Get-Date).ToString('s'); error = $why; shortcut = $Shortcut
      shortcut_exists = (Test-Path -LiteralPath $Shortcut)
      target = $link.TargetPath; target_exists = [bool]($link.TargetPath -and (Test-Path -LiteralPath $link.TargetPath))
      arguments = $link.Arguments; workdir = $link.WorkingDirectory
      workdir_exists = [bool]($link.WorkingDirectory -and (Test-Path -LiteralPath $link.WorkingDirectory))
      minutes_since_boot = $before.minutes_since_boot
    }
    ($seen | ConvertTo-Json -Compress) | Out-File -Append -Encoding utf8 (Join-Path (Split-Path -Parent $Out) 'launch-failures.jsonl')
    # Two minutes after a boot the shortcut was not there at all (shortcut_exists false), and it
    # is again later with its old timestamp. Then use what it holds: the installed tC4 and its
    # app folder, as the shortcut names them.
    $home4 = "$env:LOCALAPPDATA\Programs\translationCore4"
    $target = if ($link.TargetPath) { $link.TargetPath } else { "$home4\electronite\electron.exe" }
    $arguments = if ($link.Arguments) { $link.Arguments } else { "`"$home4\electron`"" }
    $workdir = if ($link.WorkingDirectory) { $link.WorkingDirectory } else { $home4 }
    if (-not (Test-Path -LiteralPath $target)) { throw "tC4 did not start: $why, and its program $target is missing too." }
    $start = @{ FilePath = $target; ArgumentList = $arguments; WorkingDirectory = $workdir; ErrorAction = 'Stop' }
    $sw.Restart()
    Start-Process @start
    $launch = 'target (the shortcut failed)'
  }
  while ($sw.Elapsed.TotalSeconds -lt $Timeout -and $null -eq $t.painted_ms) {
    $win = Get-Window electron
    if ($win) {
      if ($null -eq $t.window_ms) { $t.window_ms = $sw.ElapsedMilliseconds }
      try { if (Test-Painted $win.Rect) { $t.painted_ms = $sw.ElapsedMilliseconds } } catch { }
    }
    Start-Sleep -Milliseconds 15
  }
}
$t.home_ms = Wait-For $homeReady $sw
if ($null -ne $t.home_ms) {
  $win = Get-Window electron
  $resume = & $hasResume $win.Rect
  $clickHome = $sw.ElapsedMilliseconds
  Invoke-Click $win.Rect 0.196 $(if ($resume) { 0.675 } else { 0.481 })
  $t.project_ms = Wait-For $projectReady $sw
  if ($null -ne $t.project_ms) {
    $clickTranslate = $sw.ElapsedMilliseconds
    Invoke-Click $win.Rect 0.461 0.051
    $t.translate_ms = Wait-For $translateReady $sw
    if ($null -ne $t.translate_ms) { $t.text_ms = Wait-For $textReady $sw }
  }
}
$row = [pscustomobject]@{
  label = $Label; at = (Get-Date).ToString('s'); launched = (-not $NoLaunch); launch = $launch
  window_ms = $t.window_ms; painted_ms = $t.painted_ms; home_ms = $t.home_ms
  home_has_resume = $resume; psalms_click_ms = $clickHome; project_ms = $t.project_ms
  translate_click_ms = $clickTranslate; translate_ms = $t.translate_ms
  text_ms = $t.text_ms
  available_mb_before = $before.available_mb_before; builds_running = $before.builds_running; minutes_since_boot = $before.minutes_since_boot
}
($row | ConvertTo-Json -Compress) | Out-File -Append -Encoding utf8 $Out
$row | ConvertTo-Json
