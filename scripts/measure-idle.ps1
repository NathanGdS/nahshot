<#
.SYNOPSIS
    Measures the memory footprint of a running nahshot and all its descendants.

.DESCRIPTION
    Finds the `nahshot` process, walks every descendant through
    Win32_Process.ParentProcessId (WebView2 children included) and sums
    PrivatePageCount and WorkingSetSize. Exits with 1 when private bytes
    exceed the idle budget from the spec (30 MB), 2 when nahshot is not running.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\measure-idle.ps1
#>
[CmdletBinding()]
param(
    [string]$ProcessName = 'nahshot',
    [double]$LimitMB = 30
)

$all = @(Get-CimInstance Win32_Process)
$roots = @($all | Where-Object { $_.Name -ieq "$ProcessName.exe" })
if ($roots.Count -eq 0) {
    Write-Error "$ProcessName is not running"
    exit 2
}

# Breadth-first walk from every nahshot process down to all descendants.
$byParent = $all | Group-Object ParentProcessId -AsHashTable -AsString
$seen = @{}
$queue = [System.Collections.Generic.Queue[object]]::new()
foreach ($root in $roots) { $queue.Enqueue($root) }
while ($queue.Count -gt 0) {
    $proc = $queue.Dequeue()
    if ($seen.ContainsKey($proc.ProcessId)) { continue }
    $seen[$proc.ProcessId] = $proc
    $children = $byParent["$($proc.ProcessId)"]
    foreach ($child in $children) {
        # PIDs are recycled: a child must have started after its parent.
        if ($child.CreationDate -ge $proc.CreationDate) { $queue.Enqueue($child) }
    }
}

$procs = $seen.Values
$privateMB = ($procs | Measure-Object PrivatePageCount -Sum).Sum / 1MB
$workingSetMB = ($procs | Measure-Object WorkingSetSize -Sum).Sum / 1MB

$culture = [System.Globalization.CultureInfo]::InvariantCulture
Write-Output ([string]::Format($culture,
        '{0} idle: private={1:F1} MB, working_set={2:F1} MB, processes={3}',
        $ProcessName, $privateMB, $workingSetMB, $procs.Count))

if ($privateMB -gt $LimitMB) { exit 1 }
exit 0
