<#
.SYNOPSIS
  Runs a program with no console window and no focus change, for automated checks.

.DESCRIPTION
  Launching a console program with Start-Process makes Windows open a console
  window, and that window grabs the foreground. This starts the process with
  CreateNoWindow so none is created. The engine's own window is already opened
  unfocused in "quiet" mode (see EngineConfig::quiet), which automated runs
  (anything with IANUS_EXIT_AFTER_FRAMES or a screenshot) turn on by default.

  Also reports whether the foreground window changed while it ran.

.EXAMPLE
  .\tools\quiet-run.ps1 -Exe target\debug\examples\room.exe -TimeoutSeconds 60
#>
param(
  [Parameter(Mandatory)] [string] $Exe,
  [string[]] $Arguments = @(),
  [string] $WorkingDirectory = (Get-Location).Path,
  [int] $TimeoutSeconds = 90
)

Add-Type @"
using System; using System.Runtime.InteropServices; using System.Text;
public class IanusFg {
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
}
"@

function Get-Foreground {
  $h = [IanusFg]::GetForegroundWindow()
  $sb = New-Object System.Text.StringBuilder 256
  [void][IanusFg]::GetWindowText($h, $sb, 256)
  "$h | $($sb.ToString())"
}

$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = (Resolve-Path $Exe).Path
$psi.Arguments = ($Arguments -join " ")
$psi.WorkingDirectory = $WorkingDirectory
$psi.UseShellExecute = $false
$psi.CreateNoWindow = $true
$psi.RedirectStandardOutput = $true
$psi.RedirectStandardError = $true

$before = Get-Foreground
$p = [System.Diagnostics.Process]::Start($psi)
$out = $p.StandardOutput.ReadToEndAsync()
$err = $p.StandardError.ReadToEndAsync()

$changes = @()
$last = $before
$deadline = (Get-Date).AddSeconds($TimeoutSeconds)
while (-not $p.HasExited) {
  Start-Sleep -Milliseconds 100
  $now = Get-Foreground
  if ($now -ne $last) { $changes += $now; $last = $now }
  if ((Get-Date) -gt $deadline) { $p.Kill(); Write-Warning "timed out after $TimeoutSeconds s, killed"; break }
}
$p.WaitForExit()

[pscustomobject]@{
  ExitCode           = $p.ExitCode
  ForegroundBefore   = $before
  ForegroundAfter    = Get-Foreground
  ForegroundChanges  = $changes
  StdErr             = $err.Result
  StdOut             = $out.Result
}
