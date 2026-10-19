# Task 5 step 2: the saved Psalms .docx open in Word as a person opens it (double-click), scrolled
# to the end and back; then every WINWORD process is measured. If Word already has the file open
# (after its first-run dialog), it is used as it is.
$ErrorActionPreference = 'Stop'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$docx = 'C:\tc4-g0\memory\Psalms-ULT.docx'
if (-not (Get-Process WINWORD -ErrorAction SilentlyContinue)) { Start-Process -FilePath $docx }

# Word refuses calls while it is busy (RPC_E_CALL_REJECTED): try again for up to two minutes.
function Retry([scriptblock] $call) {
  for ($i = 0; ; $i++) {
    try { return & $call } catch {
      if ($i -ge 120 -or $_.Exception.HResult -notin @(-2147418111, -2147417846)) { throw }
      Start-Sleep -Seconds 1
    }
  }
}
$word = $null
for ($i = 0; $i -lt 60 -and -not $word; $i++) {
  try { $word = [Runtime.InteropServices.Marshal]::GetActiveObject('Word.Application') } catch { Start-Sleep -Seconds 1 }
}
if (-not $word) { throw 'Word did not start.' }
for ($i = 0; $i -lt 60 -and (Retry { $word.Documents.Count }) -lt 1; $i++) { Start-Sleep -Seconds 1 }
$doc = Retry { $word.ActiveDocument }
Start-Sleep -Seconds 5
Retry { $word.Selection.EndKey(6) | Out-Null; $word.ActiveWindow.ScrollIntoView($word.Selection.Range) }
Start-Sleep -Seconds 5
Retry { $word.Selection.HomeKey(6) | Out-Null; $word.ActiveWindow.ScrollIntoView($word.Selection.Range) }
$pages = Retry { $doc.ComputeStatistics(2) }
"Word $(Retry { $word.Version }) build $(Retry { $word.Build }); $(Retry { $doc.Name }) open; $pages pages; view type $(Retry { $word.ActiveWindow.View.Type }); navigation pane shown: $(Retry { $word.ActiveWindow.DocumentMap })"
[void][Runtime.InteropServices.Marshal]::ReleaseComObject($doc)
[void][Runtime.InteropServices.Marshal]::ReleaseComObject($word)
& (Join-Path $here 'measure.ps1') -Label 'Microsoft Word 2016, Psalms .docx open (137 pages), after scrolling to the end and back' -Instance 'WINWORD*' -Match 'WINWORD'
