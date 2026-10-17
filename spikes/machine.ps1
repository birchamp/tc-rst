# Describes the machine a spike or benchmark runs on, for docs/evidence (PERFORMANCE.md rule 5).
# Dot-source it: . "$PSScriptRoot/../machine.ps1". Works in Windows PowerShell 5.1 and in
# PowerShell 7 on Windows and Linux. It records no user name, no token and no password.

function Test-OnWindows {
    if ($PSVersionTable.PSEdition -eq 'Desktop') { return $true }
    return [bool]$IsWindows
}

function Format-Invariant([string]$Format, $Value) {
    return [string]::Format([Globalization.CultureInfo]::InvariantCulture, $Format, $Value)
}

# foldbench's machine line ("cpu: sha false, avx2 true, avx512f false, ...") as the JSON that
# `tc4-standin machine` prints.
function ConvertFrom-MachineLine([string]$Line) {
    $features = [ordered]@{}
    foreach ($name in @('sha', 'avx2', 'avx512f', 'avx512bw', 'avx512vl')) {
        $m = [regex]::Match("$Line", "\b$name (true|false)\b")
        if ($m.Success) { $features[$name] = ($m.Groups[1].Value -eq 'true') }
    }
    if ($features.Count -eq 0) { return '' }
    return ($features | ConvertTo-Json -Compress)
}

# CPU features as `tc4-standin machine` or foldbench's machine line report them.
function Get-MachineFacts([string]$CpuFeaturesJson) {
    $facts = [ordered]@{}
    $features = $null
    if ($CpuFeaturesJson) {
        try { $features = $CpuFeaturesJson | ConvertFrom-Json } catch { $features = $null }
    }
    if (Test-OnWindows) {
        try {
            $os = Get-CimInstance Win32_OperatingSystem
            $facts.OS = "$($os.Caption) $($os.Version) (build $($os.BuildNumber)), $($os.OSArchitecture)"
        } catch { $facts.OS = 'unavailable' }
        try {
            $system = Get-CimInstance Win32_ComputerSystem
            $facts.Computer = "$($system.Manufacturer) $($system.Model)"
            $facts.RAM = Format-Invariant '{0:0.0} GiB' ($system.TotalPhysicalMemory / 1GB)
        } catch { $facts.Computer = 'unavailable'; $facts.RAM = 'unavailable' }
        try {
            $cpus = @(Get-CimInstance Win32_Processor)
            $facts.CPU = ($cpus | ForEach-Object { $_.Name.Trim() }) -join '; '
            $cores = ($cpus | Measure-Object -Property NumberOfCores -Sum).Sum
            $threads = ($cpus | Measure-Object -Property NumberOfLogicalProcessors -Sum).Sum
            $facts.Cores = "$cores cores, $threads threads"
        } catch { $facts.CPU = 'unavailable'; $facts.Cores = 'unavailable' }
        try {
            $disks = @(Get-PhysicalDisk | ForEach-Object {
                    Format-Invariant '{0} ({1}, {2}, {3:0} GB)' @($_.FriendlyName, $_.MediaType, $_.BusType, ($_.Size / 1GB))
                })
            $facts.Disks = $disks -join '; '
        } catch { $facts.Disks = 'unavailable' }
        try {
            $battery = @(Get-CimInstance Win32_Battery)
            if ($battery.Count -eq 0) { $facts.Power = 'no battery (mains)' }
            elseif ($battery[0].BatteryStatus -eq 2) { $facts.Power = 'on mains power' }
            else { $facts.Power = "on battery (status $($battery[0].BatteryStatus)); results on battery are slower" }
        } catch { $facts.Power = 'unavailable' }
        try {
            $plan = (powercfg /getactivescheme) 2>$null
            if ($plan) { $facts.'Power plan' = ($plan -replace '^.*\((.*)\)\s*$', '$1') }
        } catch { $facts.'Power plan' = 'unavailable' }
        $facts.Defender = Get-DefenderState
    } else {
        $facts.OS = (uname -sr) -join ' '
        $model = (Get-Content /proc/cpuinfo | Where-Object { $_ -like 'model name*' } | Select-Object -First 1)
        $facts.CPU = if ($model) { ($model -split ':', 2)[1].Trim() } else { 'unavailable' }
        $facts.Cores = "$((Get-Content /proc/cpuinfo | Where-Object { $_ -like 'processor*' }).Count) logical CPUs"
        $memory = (Get-Content /proc/meminfo | Select-Object -First 1) -replace '[^0-9]', ''
        $facts.RAM = Format-Invariant '{0:0.0} GiB' ([double]$memory / 1MB)
        $facts.Disks = 'not read on Linux'
        $facts.Power = 'not read on Linux'
        $facts.Defender = 'not Windows'
    }
    if ($features) {
        $facts.'SHA instructions' = "$($features.sha)"
        $avx512 = @(foreach ($name in @('avx512f', 'avx512bw', 'avx512vl')) {
                if ($null -ne $features.$name) { "$($name.Substring(6).ToUpper()) $($features.$name)" }
            })
        if ($avx512.Count) { $facts.'AVX-512' = $avx512 -join ', ' }
        $facts.AVX2 = "$($features.avx2)"
    }
    return $facts
}

function Get-DefenderState {
    try {
        $status = Get-MpComputerStatus -ErrorAction Stop
    } catch {
        return 'unknown: Get-MpComputerStatus is not available (another antivirus, or Defender is off)'
    }
    $realTime = if ($status.RealTimeProtectionEnabled) { 'on' } else { 'OFF' }
    $text = "real-time protection $realTime; antivirus enabled $($status.AntivirusEnabled); mode $($status.AMRunningMode)"
    try {
        $paths = @((Get-MpPreference -ErrorAction Stop).ExclusionPath)
        if ($paths.Count -eq 0 -or ($paths.Count -eq 1 -and -not $paths[0])) {
            $text += '; no excluded folders'
        } elseif ($paths[0] -like 'N/A*') {
            $text += '; the excluded folders can be read only by an administrator'
        } else {
            $text += "; $($paths.Count) excluded folder(s)"
        }
    } catch {
        $text += '; the excluded folders could not be read'
    }
    return $text
}

function Format-MachineTable($Facts) {
    $lines = @('| | |', '|---|---|')
    foreach ($key in $Facts.Keys) {
        $value = "$($Facts[$key])" -replace '\|', '/'
        $lines += "| $key | $value |"
    }
    return $lines
}

function Get-Commit([string]$Repo) {
    try {
        $top = (git -C $Repo rev-parse --show-toplevel 2>$null)
        if (-not $top -or ((Resolve-Path $top).Path.TrimEnd('\', '/') -ne (Resolve-Path $Repo).Path.TrimEnd('\', '/'))) {
            return 'unknown (not the top of a git checkout)'
        }
        $sha = (git -C $Repo rev-parse HEAD 2>$null)
        $branch = (git -C $Repo rev-parse --abbrev-ref HEAD 2>$null)
        $dirty = (git -C $Repo status --porcelain --untracked-files=no 2>$null)
        if (-not $sha) { return 'unknown (not a git checkout)' }
        $text = "$sha on $branch"
        if ($dirty) { $text += ' (with local changes)' }
        return $text
    } catch {
        return 'unknown'
    }
}

function Write-Utf8File([string]$Path, [string]$Text) {
    $directory = Split-Path -Parent $Path
    if ($directory -and -not (Test-Path -LiteralPath $directory)) {
        New-Item -ItemType Directory -Force -Path $directory | Out-Null
    }
    [IO.File]::WriteAllText($Path, $Text, (New-Object System.Text.UTF8Encoding $false))
}

# A path as given, made absolute against PowerShell's location: .NET calls resolve a relative
# path against the process's directory, which is not always that location.
function Resolve-FullPath([string]$Path) {
    if ([IO.Path]::IsPathRooted($Path)) { return [IO.Path]::GetFullPath($Path) }
    return [IO.Path]::GetFullPath((Join-Path (Get-Location).Path $Path))
}

function Get-NowMs {
    return [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
}

# One argument for a Windows command line (CommandLineToArgvW's rules); plain on Linux too.
function ConvertTo-Argument([string]$Value) {
    if ($Value -eq '') { return '""' }
    if ($Value -notmatch '[\s"]') { return $Value }
    $escaped = $Value -replace '(\\*)"', '$1$1\"'
    $escaped = $escaped -replace '(\\+)$', '$1$1'
    return '"' + $escaped + '"'
}

function Join-Arguments([string[]]$Values) {
    return ($Values | ForEach-Object { ConvertTo-Argument $_ }) -join ' '
}
