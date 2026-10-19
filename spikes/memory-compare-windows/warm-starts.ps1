# Warm starts: each program started -Runs times, closed in between, with the laptop otherwise idle.
# A start is warm when the program ran since boot, so its files are in Windows' cache.
param([int] $Runs = 5, [string[]] $Programs = @('word', 'edge', 'tc4'), [string] $Tag = 'warm')
$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot
. (Join-Path $here 'startup-lib.ps1')
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$docx = 'C:\tc4-g0\memory\Psalms-ULT.docx'
$page = 'C:\tc4-g0\memory\psalms.html'
$edge = 'C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe'

function Close-Word {
  try { $w = [Runtime.InteropServices.Marshal]::GetActiveObject('Word.Application'); $w.Quit(0); [void][Runtime.InteropServices.Marshal]::ReleaseComObject($w) } catch { }
  for ($i = 0; $i -lt 40 -and (Get-Process WINWORD -ErrorAction SilentlyContinue); $i++) { Start-Sleep -Milliseconds 250 }
}
function Close-EdgeWindow {
  Get-Process msedge -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowTitle } | ForEach-Object { [void]$_.CloseMainWindow() }
  for ($i = 0; $i -lt 40 -and (Get-Window msedge); $i++) { Start-Sleep -Milliseconds 250 }
}
function Close-Tc4 {
  Get-Process electron -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowTitle } | ForEach-Object { [void]$_.CloseMainWindow() }
  for ($i = 0; $i -lt 60 -and (Get-Process electron, server -ErrorAction SilentlyContinue | Where-Object { $_.Path -like '*translationCore4*' }); $i++) { Start-Sleep -Milliseconds 500 }
}

$wordUsable = { param($win) $f = [Windows.Automation.AutomationElement]::FocusedElement; $f -and $f.Current.ClassName -eq '_WwG' }
$edgeUsable = { param($win) [bool](Get-Process msedge -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowTitle -like 'Psalms (ULT v91)*' }) }

for ($run = 1; $run -le $Runs; $run++) {
  foreach ($p in $Programs) {
    Start-Sleep -Seconds 10   # let the last close settle
    switch ($p) {
      'word' {
        Close-Word
        & (Join-Path $here 'startup-time.ps1') -Name WINWORD -Launch $docx -Usable $wordUsable -Label "Word 2016, Psalms .docx, $Tag start $run" | Out-Null
        Start-Sleep -Seconds 3; Close-Word
      }
      'edge' {
        Close-EdgeWindow
        & (Join-Path $here 'startup-time.ps1') -Name msedge -AllowRunning -Launch $edge -Arguments @('--new-window', $page) -Usable $edgeUsable -Label "Edge 155, Psalms page, $Tag start $run" | Out-Null
        Start-Sleep -Seconds 3; Close-EdgeWindow
      }
      'tc4' {
        Close-Tc4
        & (Join-Path $here 'tc4-time.ps1') -Label "tC4 4.0.0-rc.2, Psalms project, $Tag start $run" | Out-Null
        Start-Sleep -Seconds 3
        # Back to Home before closing, so the next start opens on Home.
        $win = Get-Window electron
        if ($win) { Invoke-Click $win.Rect 0.072 0.04; Start-Sleep -Seconds 2 }
        Close-Tc4
      }
    }
  }
}
Get-Content 'C:\tc4-g0\memory\startup.jsonl' -Encoding UTF8 | Select-Object -Last ($Runs * $Programs.Count)
