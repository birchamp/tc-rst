# The throwaway Psalms project for tC4 4.0's measurements, made through the running tC4's own
# local API (as its post-install smoke test makes one): a new text translation with Psalms, whose
# PSA.usfm is then the bundled ULT v91 Psalms, alignments included, so it reads like a finished,
# aligned Psalms. tC4 must be running (it serves 127.0.0.1:19119-19139).
#
# Two changes to the ULT file, both needed for tC4 to read it all:
# - Unicode NFC. tC4 refuses to open a project whose USFM is not, with "fold-of-seed differs from
#   disk bytes (is the file NFC?)"; the ULT's Hebrew alignment attributes are not NFC.
# - Every \v (and each \d superscription) starts its own line, as tC4 writes USFM itself. With the
#   ULT's "\q1 \v 1" lines, tC4 read only 22 of the 2,461 verses (21 chapters in its rail, and
#   "0 of 22 verses aligned"); with the change it reads 2,411 and lists all 150 chapters.
param(
  [string] $Ult = 'C:\tc4-g0\memory\ult\en_ult\ingredients\PSA.usfm',   # from bundle/en/en_ult_v91.sb.zip
  [string] $Abbr = 'psalms'
)
$ErrorActionPreference = 'Stop'
$base = $null
foreach ($q in 19119..19139) {
  try { $v = Invoke-RestMethod "http://127.0.0.1:$q/api/version" -TimeoutSec 1; if ($v.product_short_name -eq 'tc4') { $base = "http://127.0.0.1:$q"; break } } catch { }
}
if (-not $base) { throw 'No tC4 server answers; start tC4 first.' }
$create = @{
  content_name = 'Psalms memory test (throwaway)'; content_abbr = $Abbr; content_language_code = 'en'
  content_language_name = $null; add_book = $true; book_code = 'PSA'; book_title = 'Psalms'; book_abbr = 'Ps'
  add_cv = $true; versification = 'eng'; branch_name = $null
} | ConvertTo-Json
Invoke-RestMethod "$base/api/git/new-text-translation" -Method Post -ContentType 'application/json' -Body $create | Out-Null
$usfm = [IO.File]::ReadAllText($Ult, [Text.Encoding]::UTF8) -replace "`r`n", "`n"
$usfm = $usfm -replace '^\\id PSA [^\n]*', '\id PSA Psalms memory test (throwaway)'
$usfm = [regex]::Replace($usfm, '(?m)(?<=\S)[ \t]+(\\v \d)', "`n`$1")
$usfm = [regex]::Replace($usfm, '(?m)(?<=\S)[ \t]+(\\d )', "`n`$1")
$usfm = $usfm.Normalize([Text.NormalizationForm]::FormC)
$body = [Text.Encoding]::UTF8.GetBytes((@{ payload = $usfm } | ConvertTo-Json -Compress))
Invoke-RestMethod "$base/api/burrito/ingredient/raw/_local_/_local_/$Abbr`?ipath=PSA.usfm" -Method Post -ContentType 'application/json; charset=utf-8' -Body $body | Out-Null
"made _local_/_local_/$Abbr on $base"
