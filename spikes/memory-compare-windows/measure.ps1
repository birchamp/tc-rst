# Task 5: a program's private working set (Task Manager's Memory column), summed over all of its
# processes: wait a minute after the last action, then three readings a minute apart; median.
# usage: measure.ps1 -Label 'Word, Psalms' -Instance WINWORD* [-Instance ...] [-WaitFirst 60]
param(
  [Parameter(Mandatory)] [string] $Label,
  [Parameter(Mandatory)] [string[]] $Instance,
  # The counter instances to keep, by name without the "#n" suffix: 'msedge*' alone would also
  # catch msedgewebview2, the WebView2 runtime other programs use.
  [string] $Match = '.*',
  [int] $WaitFirst = 60,
  [int] $Gap = 60,
  [string] $Out = 'C:\tc4-g0\memory\readings.jsonl'
)
Start-Sleep -Seconds $WaitFirst
$readings = @()
for ($i = 0; $i -lt 3; $i++) {
  if ($i -gt 0) { Start-Sleep -Seconds $Gap }
  $paths = $Instance | ForEach-Object { "\Process($_)\Working Set - Private" }
  $samples = (Get-Counter -Counter $paths -ErrorAction SilentlyContinue).CounterSamples |
    Where-Object { $_.InstanceName -ne '_total' -and $_.CookedValue -gt 0 -and ($_.InstanceName -replace '#\d+$', '') -match "^($Match)$" }
  $mib = [math]::Round((($samples | Measure-Object CookedValue -Sum).Sum) / 1MB, 1)
  $avail = (Get-Counter '\Memory\Available MBytes').CounterSamples[0].CookedValue
  $busy = @(Get-Process cargo, rustc, link -ErrorAction SilentlyContinue).Count
  $readings += [pscustomobject]@{
    at = (Get-Date).ToString('s'); mib = $mib; processes = @($samples).Count; builds_running = $busy
    instances = ($samples | ForEach-Object { '{0}={1:N1}' -f $_.InstanceName, ($_.CookedValue / 1MB) }) -join ' '
    available_mb = $avail
  }
}
$median = ($readings.mib | Sort-Object)[1]
$row = [pscustomobject]@{ label = $Label; instance = ($Instance -join ','); median_mib = $median; readings = $readings }
($row | ConvertTo-Json -Depth 4 -Compress) | Out-File -Append -Encoding utf8 $Out
$row | ConvertTo-Json -Depth 4
