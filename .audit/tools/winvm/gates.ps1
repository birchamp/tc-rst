# The workspace gates on Windows (MSVC), run in C:\src\tc-rst by run.sh. Each gate runs through
# cmd so cargo's progress on stderr is not taken for a PowerShell error; its log goes to C:\src\logs.
$logs = 'C:\src\logs'; New-Item -ItemType Directory -Force $logs | Out-Null
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
$env:RUST_TEST_THREADS = '4'
# The VM's 80 GB disk filled with incremental caches; a gate run gains little from them.
$env:CARGO_INCREMENTAL = '0'
$gates = [ordered]@{
  fmt    = 'cargo fmt --all --check'
  clippy = 'cargo clippy --locked --workspace --all-targets --keep-going -- -D warnings'
  test   = 'cargo test --locked --workspace --no-fail-fast --exclude tc-app'
  'test-app' = 'cargo test --locked -p tc-app --no-fail-fast'
  'wasm-ui' = 'cargo clippy --locked -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings'
}

$failed = @()
foreach ($g in $gates.Keys) {
  $t = [Diagnostics.Stopwatch]::StartNew()
  cmd /c "$($gates[$g]) > $logs\$g.log 2>&1"
  $code = $LASTEXITCODE; $t.Stop()
  Write-Output ("{0,-7} exit {1}  {2:N0} s" -f $g, $code, $t.Elapsed.TotalSeconds)
  if ($code -ne 0) { $failed += $g }
}
$r = Select-String -Path "$logs\test.log","$logs\test-app.log" -Pattern '^test result:' | ForEach-Object { $_.Line }
$p = ($r | ForEach-Object { if ($_ -match '(\d+) passed') { [int]$Matches[1] } } | Measure-Object -Sum).Sum
$f = ($r | ForEach-Object { if ($_ -match '(\d+) failed') { [int]$Matches[1] } } | Measure-Object -Sum).Sum
Write-Output "tests: $p passed, $f failed"
foreach ($g in $failed) {
  Write-Output "---- $g"
  Select-String -CaseSensitive -Path "$logs\$g.log" -Pattern '^error','\.\.\. FAILED','panicked at' | Select-Object -First 30 | ForEach-Object { $_.Line }
}
