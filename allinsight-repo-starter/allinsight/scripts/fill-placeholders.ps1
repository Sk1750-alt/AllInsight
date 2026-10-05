# Replaces the placeholders OWNER, FULL_NAME and YOUR-DOMAIN across the repository.
# Usage (from the repository root):
#   .\scripts\fill-placeholders.ps1 -Owner your-github-name -FullName "Your Full Name" -Domain allinsight.example
param(
    [Parameter(Mandatory)] [string]$Owner,
    [Parameter(Mandatory)] [string]$FullName,
    [Parameter(Mandatory)] [string]$Domain
)

$root = Split-Path -Parent $PSScriptRoot
$skipNames = @('LICENSE', 'GPL-3.0-or-later.txt', 'fill-placeholders.ps1')
$textExtensions = @('.md', '.yml', '.yaml', '.toml', '.txt')

Get-ChildItem $root -Recurse -File -Force |
    Where-Object {
        $_.FullName -notmatch '\\(\.git|node_modules|target)\\' -and
        $skipNames -notcontains $_.Name -and
        ($textExtensions -contains $_.Extension -or $_.Name -eq 'CODEOWNERS')
    } |
    ForEach-Object {
        $text = [IO.File]::ReadAllText($_.FullName)
        $new = $text -creplace '\bOWNER\b', $Owner
        $new = $new.Replace('FULL_NAME', $FullName).Replace('YOUR-DOMAIN', $Domain)
        if ($new -ne $text) {
            [IO.File]::WriteAllText($_.FullName, $new)
            Write-Host "Updated $($_.FullName.Substring($root.Length + 1))"
        }
    }

$left = Get-ChildItem $root -Recurse -File -Force |
    Where-Object { $_.FullName -notmatch '\\(\.git|node_modules|target)\\' -and $_.Name -ne 'fill-placeholders.ps1' -and $_.Name -ne 'MAINTAINER_SETUP.md' } |
    Select-String -Pattern '\bOWNER\b', 'FULL_NAME', 'YOUR-DOMAIN' -CaseSensitive
if ($left) { Write-Warning "Placeholders still present:"; $left | ForEach-Object { Write-Host "  $($_.Path):$($_.LineNumber)" } }
else { Write-Host "All placeholders replaced." }
