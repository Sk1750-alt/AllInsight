# Drive the first-run wizard from the keyboard so the Overview can be
# inspected. Development helper only; nothing in the application depends on it.

Add-Type -AssemblyName System.Windows.Forms

Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Fg {
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr hWnd);
}
"@

$process = Get-Process -Name "allinsight" -ErrorAction SilentlyContinue |
    Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $process) { Write-Error "AllInsight is not running."; exit 1 }

$handle = $process.MainWindowHandle
[Fg]::ShowWindow($handle, 9) | Out-Null
[Fg]::BringWindowToTop($handle) | Out-Null
[Fg]::SetForegroundWindow($handle) | Out-Null
Start-Sleep -Milliseconds 1500

# Each step: Tab to the primary button, then activate it. The Back button is
# disabled on the first step and skipped by the focus order.
for ($step = 0; $step -lt 4; $step++) {
    [System.Windows.Forms.SendKeys]::SendWait("{TAB}")
    Start-Sleep -Milliseconds 400
    [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
    Start-Sleep -Milliseconds 1600
    Write-Output "step $step advanced"
}

Start-Sleep -Seconds 3
Write-Output "done"
