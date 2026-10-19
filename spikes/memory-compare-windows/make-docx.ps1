# Task 5: open the Psalms HTML in Word, save it as .docx in Print Layout, report pages and size, quit.
$ErrorActionPreference = 'Stop'
$html = 'C:\tc4-g0\memory\psalms.html'
$docx = 'C:\tc4-g0\memory\Psalms-ULT.docx'
Remove-Item $docx -ErrorAction SilentlyContinue
$word = New-Object -ComObject Word.Application
$word.Visible = $false
try {
  $doc = $word.Documents.Open($html, $false, $true)
  $doc.ActiveWindow.View.Type = 3          # wdPrintView
  $doc.SaveAs2($docx, 16)                  # wdFormatDocumentDefault (.docx)
  $doc.Repaginate()
  $pages = $doc.ComputeStatistics(2)       # wdStatisticPages
  $words = $doc.ComputeStatistics(0)       # wdStatisticWords
  $paras = $doc.Paragraphs.Count
  $doc.Close($false)
} finally {
  $word.Quit()
  [void][Runtime.InteropServices.Marshal]::ReleaseComObject($word)
}
[pscustomobject]@{ docx = $docx; bytes = (Get-Item $docx).Length; pages = $pages; words = $words; paragraphs = $paras; word_version = (Get-Item 'C:\Program Files (x86)\Microsoft Office\Office16\WINWORD.EXE').VersionInfo.ProductVersion } | ConvertTo-Json
