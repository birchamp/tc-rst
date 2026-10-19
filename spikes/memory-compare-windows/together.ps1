# Task 5 step 6: Word (Psalms) and Edge (Psalms) open together; how much memory the laptop has
# left. Also each program's share at that moment, and what this session's own Claude app holds
# (a translator would not have it open, so it is reported apart).
$ErrorActionPreference = 'Stop'
Start-Sleep -Seconds 60
$rows = @()
for ($i = 0; $i -lt 3; $i++) {
  if ($i -gt 0) { Start-Sleep -Seconds 60 }
  $sum = {
    param($pattern, $match)
    $s = (Get-Counter "\Process($pattern)\Working Set - Private" -ErrorAction SilentlyContinue).CounterSamples |
      Where-Object { ($_.InstanceName -replace '#\d+$', '') -match "^($match)$" }
    [math]::Round((($s | Measure-Object CookedValue -Sum).Sum) / 1MB, 1)
  }
  $rows += [pscustomobject]@{
    at = (Get-Date).ToString('s')
    available_mb = (Get-Counter '\Memory\Available MBytes').CounterSamples[0].CookedValue
    committed_mb = [math]::Round((Get-Counter '\Memory\Committed Bytes').CounterSamples[0].CookedValue / 1MB)
    commit_limit_mb = [math]::Round((Get-Counter '\Memory\Commit Limit').CounterSamples[0].CookedValue / 1MB)
    word_mib = & $sum 'WINWORD*' 'WINWORD'
    edge_mib = & $sum 'msedge*' 'msedge'
    claude_app_mib = & $sum 'claude*' 'claude'
    builds_running = @(Get-Process cargo, rustc, link -ErrorAction SilentlyContinue).Count
  }
}
$ram = (Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory
$result = [pscustomobject]@{ label = 'Everything together: Word (Psalms) and Edge (Psalms)'; total_ram_mb = [math]::Round($ram / 1MB); readings = $rows }
($result | ConvertTo-Json -Depth 4 -Compress) | Out-File -Append -Encoding utf8 'C:\tc4-g0\memory\readings.jsonl'
$result | ConvertTo-Json -Depth 4
