# The workspace gates on Windows (MSVC), run in C:\src\tc-rst by run.sh. Each gate runs through
# cmd so cargo's progress on stderr is not taken for a PowerShell error; its log goes to C:\src\logs.
$logs = 'C:\src\logs'; New-Item -ItemType Directory -Force $logs | Out-Null
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
$env:RUST_TEST_THREADS = '4'; $env:NEXTEST_TEST_THREADS = '4'
# The test gates run through cargo-nextest (CHK-8), pinned as CI pins it.
if (-not (Get-Command cargo-nextest -ErrorAction SilentlyContinue)) { cmd /c "cargo install cargo-nextest --version 0.9.149 --locked > $logs\nextest-install.log 2>&1" }
# The VM's 80 GB disk filled with incremental caches; a gate run gains little from them.
$env:CARGO_INCREMENTAL = '0'
$gates = [ordered]@{
  fmt    = 'cargo fmt --all --check'
  clippy = 'cargo clippy --locked --workspace --all-targets --keep-going -- -D warnings'
  test   = 'cargo nextest run --locked --workspace --no-fail-fast --exclude tc-app && cargo test --locked --workspace --doc --no-fail-fast --exclude tc-app'
  'test-app' = 'cargo nextest run --locked -p tc-app --no-fail-fast && cargo test --locked -p tc-app --test op_counts --no-fail-fast && cargo test --locked -p tc-app --doc --no-fail-fast'
  'wasm-ui' = 'cargo clippy --locked -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings'
}

$failed = @()
foreach ($g in $gates.Keys) {
  $t = [Diagnostics.Stopwatch]::StartNew()
  cmd /c "($($gates[$g])) > $logs\$g.log 2>&1"
  $code = $LASTEXITCODE; $t.Stop()
  Write-Output ("{0,-7} exit {1}  {2:N0} s" -f $g, $code, $t.Elapsed.TotalSeconds)
  if ($code -ne 0) { $failed += $g }
}
$r = Select-String -Path "$logs\test.log","$logs\test-app.log" -Pattern '^test result:','^\s+Summary \[' | ForEach-Object { $_.Line }
$p = ($r | ForEach-Object { if ($_ -match '(\d+) passed') { [int]$Matches[1] } } | Measure-Object -Sum).Sum
$f = ($r | ForEach-Object { if ($_ -match '(\d+) failed') { [int]$Matches[1] } } | Measure-Object -Sum).Sum
Write-Output "tests: $p passed, $f failed"
foreach ($g in $failed) {
  Write-Output "---- $g"
  Select-String -CaseSensitive -Path "$logs\$g.log" -Pattern '^error','\.\.\. FAILED','panicked at','^\s+(FAIL|TIMEOUT|SIGSEGV|SIGABRT) \[' | Select-Object -First 30 | ForEach-Object { $_.Line }
}
