# Capture the AllInsight window to a PNG.
#
# Used during development to check that a screen actually renders, since a
# blank panel caused by a failed IPC call looks identical to a slow one in
# every other kind of log.
#
#   powershell -ExecutionPolicy Bypass -File scripts\screenshot.ps1 out.png
#
# Uses PrintWindow with PW_RENDERFULLCONTENT rather than a screen grab, because
# a screen grab captures whatever is in front of the window - and Windows will
# not always let a background process raise itself to the foreground.

param(
    [string]$Output = "allinsight-screenshot.png",
    [string]$ProcessName = "allinsight"
)

Add-Type -AssemblyName System.Drawing

Add-Type @"
using System;
using System.Runtime.InteropServices;
public class WinCap {
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdcBlt, uint nFlags);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }
}
"@

# Without this, GetWindowRect reports virtualised coordinates on a scaled
# display and the capture comes back cropped and magnified.
[WinCap]::SetProcessDPIAware() | Out-Null

$process = Get-Process -Name $ProcessName -ErrorAction SilentlyContinue |
    Where-Object { $_.MainWindowHandle -ne 0 } |
    Select-Object -First 1

if (-not $process) {
    Write-Error "No $ProcessName window found. Is the application running?"
    exit 1
}

$handle = $process.MainWindowHandle
# SW_SHOW then SW_RESTORE: a window hidden to the notification area needs the
# first, a minimised one needs the second, and AllInsight can be in either state.
[WinCap]::ShowWindow($handle, 5) | Out-Null   # SW_SHOW
[WinCap]::ShowWindow($handle, 9) | Out-Null   # SW_RESTORE
[WinCap]::SetForegroundWindow($handle) | Out-Null
Start-Sleep -Milliseconds 1200

$rect = New-Object WinCap+RECT
[WinCap]::GetWindowRect($handle, [ref]$rect) | Out-Null
$width = $rect.Right - $rect.Left
$height = $rect.Bottom - $rect.Top

if ($width -le 0 -or $height -le 0) {
    Write-Error "The window reported a zero size."
    exit 1
}

$bitmap = New-Object System.Drawing.Bitmap $width, $height
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$hdc = $graphics.GetHdc()

# PW_RENDERFULLCONTENT (0x2) is what makes this work for a WebView2 surface.
$ok = [WinCap]::PrintWindow($handle, $hdc, 2)
$graphics.ReleaseHdc($hdc)

if (-not $ok) {
    # Fall back to a screen grab of the window rectangle.
    $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
}

$bitmap.Save($Output, [System.Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose()
$bitmap.Dispose()

Write-Output "Saved $Output ($width x $height, PrintWindow=$ok, visible=$([WinCap]::IsWindowVisible($handle)))"
