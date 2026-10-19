# Task 5 step 6: open Word (Psalms) and Edge (Psalms) as before, scroll both, then together.ps1.
$ErrorActionPreference = 'Stop'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
function Retry([scriptblock] $call) {
  for ($i = 0; ; $i++) {
    try { return & $call } catch {
      if ($i -ge 120 -or $_.Exception.HResult -notin @(-2147418111, -2147417846, -2147221021)) { throw }
      Start-Sleep -Seconds 1
    }
  }
}
Start-Process -FilePath 'C:\tc4-g0\memory\Psalms-ULT.docx'
Start-Sleep -Seconds 5
$word = Retry { [Runtime.InteropServices.Marshal]::GetActiveObject('Word.Application') }
for ($i = 0; $i -lt 60 -and (Retry { $word.Documents.Count }) -lt 1; $i++) { Start-Sleep -Seconds 1 }
Start-Sleep -Seconds 5
Retry { $word.Selection.EndKey(6) | Out-Null; $word.ActiveWindow.ScrollIntoView($word.Selection.Range) }
Start-Sleep -Seconds 5
Retry { $word.Selection.HomeKey(6) | Out-Null; $word.ActiveWindow.ScrollIntoView($word.Selection.Range) }
"word: $(Retry { $word.ActiveDocument.Name })"
[void][Runtime.InteropServices.Marshal]::ReleaseComObject($word)
Start-Process -FilePath 'C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe' -ArgumentList '--new-window', 'file:///C:/tc4-g0/memory/psalms.html'
Start-Sleep -Seconds 10
$shell = New-Object -ComObject WScript.Shell
if ($shell.AppActivate('Psalms (ULT v91)')) { Start-Sleep -Milliseconds 500; $shell.SendKeys('^{END}'); Start-Sleep -Seconds 3; $shell.SendKeys('^{HOME}'); 'edge: scrolled' } else { 'edge: window not found' }
& (Join-Path $here 'together.ps1')
