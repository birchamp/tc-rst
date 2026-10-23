# Shared by the startup-timing scripts: finding a program's window, telling a painted window from a
# blank one, and finding a colour where a known control sits. Dot-source it.
Add-Type -AssemblyName System.Drawing
if (-not ('StartupProbe.Win' -as [type])) {
  Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
namespace StartupProbe {
  public static class Win {
    delegate bool EnumProc(IntPtr h, IntPtr l);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc f, IntPtr l);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, int d, UIntPtr e);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
    public static IntPtr FirstWindow(HashSet<uint> pids, int minW, int minH, out RECT rect) {
      IntPtr found = IntPtr.Zero; RECT best = new RECT();
      EnumWindows((h, l) => {
        uint pid; GetWindowThreadProcessId(h, out pid);
        if (!pids.Contains(pid) || !IsWindowVisible(h)) return true;
        RECT r; if (!GetWindowRect(h, out r)) return true;
        if (r.R - r.L >= minW && r.B - r.T >= minH) { found = h; best = r; return false; }
        return true;
      }, IntPtr.Zero);
      rect = best; return found;
    }
    public static void Click(int x, int y) { SetCursorPos(x, y); mouse_event(2, 0, 0, 0, UIntPtr.Zero); mouse_event(4, 0, 0, 0, UIntPtr.Zero); }
  }
}
'@
}
# Screen pixels, not scaled ones, so window rectangles and captures agree at any display scale.
[void][StartupProbe.Win]::SetProcessDPIAware()

function Get-Pids([string] $Name) {
  $set = New-Object 'System.Collections.Generic.HashSet[uint32]'
  Get-Process -Name $Name -ErrorAction SilentlyContinue | ForEach-Object { [void]$set.Add([uint32]$_.Id) }
  , $set
}

function Get-Window([string] $Name) {
  $rect = New-Object StartupProbe.Win+RECT
  $h = [StartupProbe.Win]::FirstWindow((Get-Pids $Name), 200, 150, [ref] $rect)
  if ($h -eq [IntPtr]::Zero) { return $null }
  [pscustomobject]@{ Handle = $h; Rect = $rect }
}

# A part of a window, given as fractions of its width and height, captured from the screen.
function Get-Area($rect, [double] $fx0, [double] $fy0, [double] $fx1, [double] $fy1) {
  $w = $rect.R - $rect.L; $h = $rect.B - $rect.T
  $x0 = $rect.L + [int]($w * $fx0); $y0 = $rect.T + [int]($h * $fy0)
  $cw = [Math]::Max(1, [int]($w * ($fx1 - $fx0))); $ch = [Math]::Max(1, [int]($h * ($fy1 - $fy0)))
  $bmp = New-Object System.Drawing.Bitmap $cw, $ch
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($x0, $y0, 0, 0, $bmp.Size)
  $g.Dispose()
  $bmp
}

# The window shows a picture rather than one flat colour: a 32 x 32 grid inside its frame
# (below a title bar) holds at least 12 distinct colours.
function Test-Painted($rect) {
  $bmp = Get-Area $rect 0.01 0.05 0.99 0.98
  try {
    $colours = New-Object 'System.Collections.Generic.HashSet[int]'
    for ($i = 0; $i -lt 32; $i++) { for ($j = 0; $j -lt 32; $j++) {
      [void]$colours.Add($bmp.GetPixel([int]($i * ($bmp.Width - 1) / 31), [int]($j * ($bmp.Height - 1) / 31)).ToArgb())
    } }
    $colours.Count -ge 12
  } finally { $bmp.Dispose() }
}

# At least $Share of a 3-pixel grid over the area is within $Tolerance of the colour #RRGGBB.
function Test-Colour($rect, [double] $fx0, [double] $fy0, [double] $fx1, [double] $fy1, [string] $Hex, [double] $Share = 0.3, [int] $Tolerance = 14) {
  $r = [Convert]::ToInt32($Hex.Substring(1, 2), 16); $g = [Convert]::ToInt32($Hex.Substring(3, 2), 16); $b = [Convert]::ToInt32($Hex.Substring(5, 2), 16)
  $bmp = Get-Area $rect $fx0 $fy0 $fx1 $fy1
  try {
    $hit = 0; $all = 0
    for ($x = 0; $x -lt $bmp.Width; $x += 3) { for ($y = 0; $y -lt $bmp.Height; $y += 3) {
      $c = $bmp.GetPixel($x, $y); $all++
      if ([Math]::Abs($c.R - $r) -le $Tolerance -and [Math]::Abs($c.G - $g) -le $Tolerance -and [Math]::Abs($c.B - $b) -le $Tolerance) { $hit++ }
    } }
    ($all -gt 0) -and ($hit / $all -ge $Share)
  } finally { $bmp.Dispose() }
}

# Click a point given as fractions of the window.
function Invoke-Click($rect, [double] $fx, [double] $fy) {
  [StartupProbe.Win]::Click($rect.L + [int](($rect.R - $rect.L) * $fx), $rect.T + [int](($rect.B - $rect.T) * $fy))
}

function Get-Conditions {
  [pscustomobject]@{
    available_mb_before = (Get-Counter '\Memory\Available MBytes').CounterSamples[0].CookedValue
    builds_running = @(Get-Process cargo, rustc, link -ErrorAction SilentlyContinue).Count
    minutes_since_boot = [math]::Round(((Get-Date) - (Get-CimInstance Win32_OperatingSystem).LastBootUpTime).TotalMinutes, 1)
  }
}
