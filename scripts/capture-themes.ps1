# Capture the Settings screen once per theme.
#
# Development helper. Each theme is applied by writing the setting and
# restarting, because synthesised input into the window is refused when the
# shell and the application run at different integrity levels.

param([string]$OutDir = (Join-Path (Split-Path -Parent $PSScriptRoot) "shots\themes"))

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class T {
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint f);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int n);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }
}
"@

[T]::SetProcessDPIAware() | Out-Null
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$exe = "C:\Program Files\AllInsight\allinsight.exe"
$db  = Join-Path $env:LOCALAPPDATA "AllInsight\allinsight.db"
$themes = @("dark", "light", "midnight", "contrast", "paper")

foreach ($theme in $themes) {
    Get-Process -Name allinsight, AllInsight -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 1200

    $py = @"
import sqlite3, json
con = sqlite3.connect(r'$db')
row = con.execute("SELECT value FROM settings WHERE key='settings.v1'").fetchone()
s = json.loads(row[0]) if row else {}
s['theme'] = '$theme'
s['first_run_complete'] = True
s['ai_load_automatically'] = False
con.execute("INSERT INTO settings (key,value) VALUES ('settings.v1',?) "
            "ON CONFLICT(key) DO UPDATE SET value=excluded.value", (json.dumps(s),))
con.execute("INSERT INTO settings (key,value) VALUES ('ui.last_route','settings') "
            "ON CONFLICT(key) DO UPDATE SET value=excluded.value")
con.commit(); con.close()
"@
    $py | python -
    if ($LASTEXITCODE -ne 0) { Write-Warning "could not set theme $theme"; continue }

    Start-Process $exe
    Start-Sleep -Seconds 11

    $proc = Get-Process -Name allinsight, AllInsight -ErrorAction SilentlyContinue |
        Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
    if (-not $proc) { Write-Warning "no window for $theme"; continue }

    $h = $proc.MainWindowHandle
    [T]::ShowWindow($h, 5) | Out-Null
    [T]::ShowWindow($h, 9) | Out-Null
    [T]::SetForegroundWindow($h) | Out-Null
    Start-Sleep -Milliseconds 1200

    $r = New-Object T+RECT
    [T]::GetWindowRect($h, [ref]$r) | Out-Null
    $w = $r.Right - $r.Left; $ht = $r.Bottom - $r.Top
    if ($w -le 0 -or $ht -le 0) { Write-Warning "zero-size window for $theme"; continue }

    $bmp = New-Object System.Drawing.Bitmap $w, $ht
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $hdc = $g.GetHdc()
    [T]::PrintWindow($h, $hdc, 2) | Out-Null
    $g.ReleaseHdc($hdc)
    $bmp.Save((Join-Path $OutDir "$theme.png"), [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    Write-Output "captured $theme ($w x $ht)"
}

Get-Process -Name allinsight, AllInsight -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Write-Output "done"
