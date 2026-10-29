# One-time setup of the Windows 11 test VM for the tc-rst rebuild (owner approved the
# Visual Studio Build Tools license and these installs, 2026-10-19). Run as the VM's admin user.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$dl = "$env:TEMP\tc-setup"; New-Item -ItemType Directory -Force $dl | Out-Null

function Get-File($url, $out) { Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $out }

# Visual Studio 2022 Build Tools with the C++ workload: the MSVC toolchain the release uses.
if (-not (Test-Path 'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC')) {
  Get-File 'https://aka.ms/vs/17/release/vs_BuildTools.exe' "$dl\vs_BuildTools.exe"
  $p = Start-Process "$dl\vs_BuildTools.exe" -Wait -PassThru -ArgumentList @('--quiet','--wait','--norestart','--nocache',
    '--add','Microsoft.VisualStudio.Workload.VCTools','--includeRecommended')
  if ($p.ExitCode -notin 0, 3010) { throw "Build Tools exit $($p.ExitCode)" }
}

# Git for Windows, long paths on.
if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
  $rel = Invoke-RestMethod 'https://api.github.com/repos/git-for-windows/git/releases/latest'
  $asset = $rel.assets | Where-Object { $_.name -match '^Git-.*-64-bit\.exe$' } | Select-Object -First 1
  Get-File $asset.browser_download_url "$dl\git.exe"
  Start-Process "$dl\git.exe" -Wait -ArgumentList '/VERYSILENT','/NORESTART','/NOCANCEL','/SP-'
  $env:Path += ';C:\Program Files\Git\cmd'
}
git config --global core.longpaths true
git config --global user.name Claude
git config --global user.email noreply@anthropic.com

# Node 22 (the parity and smoke scripts).
if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
  $idx = Invoke-RestMethod 'https://nodejs.org/dist/index.json'
  $v = ($idx | Where-Object { $_.version -like 'v22.*' } | Select-Object -First 1).version
  Get-File "https://nodejs.org/dist/$v/node-$v-x64.msi" "$dl\node.msi"
  Start-Process msiexec -Wait -ArgumentList '/i', "$dl\node.msi", '/qn', '/norestart'
}

# Rust stable with the MSVC host, clippy, rustfmt and the wasm target.
if (-not (Test-Path "$env:USERPROFILE\.cargo\bin\cargo.exe")) {
  Get-File 'https://win.rustup.rs/x86_64' "$dl\rustup-init.exe"
  Start-Process "$dl\rustup-init.exe" -Wait -NoNewWindow -ArgumentList '-y','--default-toolchain','stable','--profile','minimal',
    '-c','clippy','-c','rustfmt','-t','wasm32-unknown-unknown'
}

# Build folders only: never a project or app-data folder (the budgets measure with Defender on).
New-Item -ItemType Directory -Force 'C:\src' | Out-Null
Add-MpPreference -ExclusionPath 'C:\src\tc-rst\target'

& "$env:USERPROFILE\.cargo\bin\rustc.exe" --version
& "$env:USERPROFILE\.cargo\bin\cargo.exe" clippy --version
git --version
node --version
Write-Output 'setup done'
