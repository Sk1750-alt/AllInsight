# Capture every screen by restarting AllInsight on each one in turn.
#
# Development helper. Windows refuses synthesised input into the application
# from a shell at a different integrity level, so navigation is driven through
# the stored last-screen value instead of by clicking.

# Paths are derived from the script's own location so the repository can live
# anywhere. They used to be absolute, which broke the moment the folder moved.
param([string]$OutDir = (Join-Path (Split-Path -Parent $PSScriptRoot) "shots"))
$RepoRoot = Split-Path -Parent $PSScriptRoot

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Cap {
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint f);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }
}
"@

[Cap]::SetProcessDPIAware() | Out-Null
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$db = Join-Path $env:LOCALAPPDATA "AllInsight\allinsight.db"
# Prefer the release binary: it is the one that ships, so it is the one the
# screenshots should show. Fall back to debug for a quick local pass.
$exe = Join-Path $RepoRoot "src-tauri\target\release\allinsight.exe"
if (-not (Test-Path $exe)) {
    $exe = Join-Path $RepoRoot "src-tauri\target\debug\allinsight.exe"
}
if (-not (Test-Path $exe)) {
    throw "No AllInsight binary found. Run BUILD_WINDOWS.bat first."
}

$screens = @(
    # Overview is the default route, so it has to be asked for explicitly like
    # the rest; without it the set is missing the screen the product leads with.
    "overview",
    "storage-map", "large-files", "duplicates", "cleanup",
    "performance", "processes", "startup", "applications",
    "battery", "drive-health", "assistant", "activity", "settings"
)

foreach ($screen in $screens) {
    Get-Process -Name allinsight -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep -Milliseconds 900

    $py = @"
import sqlite3
con = sqlite3.connect(r'$db')
con.execute("INSERT INTO settings (key,value) VALUES ('ui.last_route', ?) "
            "ON CONFLICT(key) DO UPDATE SET value=excluded.value", ('$screen',))
con.commit(); con.close()
"@
    $py | python -
    if ($LASTEXITCODE -ne 0) { Write-Warning "could not set route $screen"; continue }

    Start-Process -FilePath $exe
    Start-Sleep -Seconds 9

    $proc = Get-Process -Name allinsight -ErrorAction SilentlyContinue |
        Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
    if (-not $proc) { Write-Warning "no window for $screen"; continue }

    $r = New-Object Cap+RECT
    [Cap]::GetWindowRect($proc.MainWindowHandle, [ref]$r) | Out-Null
    $w = $r.Right - $r.Left; $h = $r.Bottom - $r.Top
    $bmp = New-Object System.Drawing.Bitmap $w, $h
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $hdc = $g.GetHdc()
    [Cap]::PrintWindow($proc.MainWindowHandle, $hdc, 2) | Out-Null
    $g.ReleaseHdc($hdc)
    $bmp.Save((Join-Path $OutDir "$screen.png"), [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    Write-Output "captured $screen"
}

Write-Output "done"
