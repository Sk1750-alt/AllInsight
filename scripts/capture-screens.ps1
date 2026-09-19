# Click each sidebar entry and capture the screen.
#
# Development helper. Uses synthesised mouse input because SendKeys is refused
# when the shell and the application run at different integrity levels.

param([string]$OutDir = (Join-Path (Split-Path -Parent $PSScriptRoot) "shots"))

Add-Type -AssemblyName System.Drawing

Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Ui {
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int X, int Y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, IntPtr e);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint f);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int n);
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }
    public const uint LEFTDOWN = 0x0002, LEFTUP = 0x0004;
}
"@

[Ui]::SetProcessDPIAware() | Out-Null
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$process = Get-Process -Name "allinsight" -ErrorAction SilentlyContinue |
    Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $process) { Write-Error "AllInsight is not running."; exit 1 }
$handle = $process.MainWindowHandle

[Ui]::ShowWindow($handle, 9) | Out-Null
[Ui]::SetForegroundWindow($handle) | Out-Null
Start-Sleep -Milliseconds 1200

$rect = New-Object Ui+RECT
[Ui]::GetWindowRect($handle, [ref]$rect) | Out-Null
$scale = 2.0   # the capture is in physical pixels; the sidebar coords below are too

function Capture([string]$name) {
    $r = New-Object Ui+RECT
    [Ui]::GetWindowRect($handle, [ref]$r) | Out-Null
    $w = $r.Right - $r.Left; $h = $r.Bottom - $r.Top
    $bmp = New-Object System.Drawing.Bitmap $w, $h
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $hdc = $g.GetHdc()
    [Ui]::PrintWindow($handle, $hdc, 2) | Out-Null
    $g.ReleaseHdc($hdc)
    $bmp.Save((Join-Path $OutDir "$name.png"), [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    Write-Output "captured $name"
}

# Sidebar item centres, in physical pixels relative to the window origin.
# Taken from the rendered layout at the default window size.
$items = @(
    @{ name = "storage-map";  y = 343 },
    @{ name = "large-files";  y = 392 },
    @{ name = "duplicates";   y = 441 },
    @{ name = "cleanup";      y = 491 },
    @{ name = "performance";  y = 596 },
    @{ name = "processes";    y = 645 },
    @{ name = "startup";      y = 695 },
    @{ name = "applications"; y = 744 },
    @{ name = "battery";      y = 794 },
    @{ name = "drive-health"; y = 865 },
    @{ name = "assistant";    y = 914 },
    @{ name = "activity";     y = 964 },
    @{ name = "settings";     y = 1013 }
)

foreach ($item in $items) {
    $x = $rect.Left + [int](150 * $scale)
    $y = $rect.Top + [int]($item.y * $scale)
    [Ui]::SetCursorPos($x, $y) | Out-Null
    Start-Sleep -Milliseconds 200
    [Ui]::mouse_event([Ui]::LEFTDOWN, 0, 0, 0, [IntPtr]::Zero)
    Start-Sleep -Milliseconds 60
    [Ui]::mouse_event([Ui]::LEFTUP, 0, 0, 0, [IntPtr]::Zero)
    Start-Sleep -Milliseconds 2200
    Capture $item.name
}

Write-Output "done"
