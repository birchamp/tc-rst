# Task 5 step 3: Edge with the same Psalms text as a local HTML page, in the owner's normal
# profile (as a translator would open it), scrolled to the end and back; every msedge process.
$ErrorActionPreference = 'Stop'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$page = 'file:///C:/tc4-g0/memory/psalms.html'
$edge = 'C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe'
Start-Process -FilePath $edge -ArgumentList '--new-window', $page
Start-Sleep -Seconds 10
$shell = New-Object -ComObject WScript.Shell
if ($shell.AppActivate('Psalms (ULT v91)')) {
  Start-Sleep -Milliseconds 500
  $shell.SendKeys('^{END}'); Start-Sleep -Seconds 3
  $shell.SendKeys('^{HOME}')
  'scrolled'
} else { 'window not found by title; not scrolled' }
$v = (Get-Item $edge).VersionInfo.ProductVersion
& (Join-Path $here 'measure.ps1') -Label "Microsoft Edge $v, the Psalms page (local HTML, the same text as the Word file), one window and one tab" -Instance 'msedge*' -Match 'msedge'
