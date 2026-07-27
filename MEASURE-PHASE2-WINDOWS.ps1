param(
    [Parameter(Mandatory = $true)]
    [string]$Executable,
    [int]$IdleSeconds = 10,
    [int]$ReadyTimeoutSeconds = 30,
    [string]$Output = "phase2-windows-performance.json"
)

$ErrorActionPreference = "Stop"
$resolved = (Resolve-Path $Executable).Path
$os = Get-CimInstance Win32_OperatingSystem
$cpu = Get-CimInstance Win32_Processor | Select-Object -First 1
$computer = Get-CimInstance Win32_ComputerSystem

function Get-ProcessTreeIds([int]$RootId) {
    $known = [System.Collections.Generic.HashSet[int]]::new()
    [void]$known.Add($RootId)
    do {
        $changed = $false
        $processes = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId
        foreach ($candidate in $processes) {
            if ($known.Contains([int]$candidate.ParentProcessId) -and $known.Add([int]$candidate.ProcessId)) {
                $changed = $true
            }
        }
    } while ($changed)
    return @($known)
}

$stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
$process = Start-Process -FilePath $resolved -PassThru
$windowReadyMs = $null
try {
    $deadline = (Get-Date).AddSeconds($ReadyTimeoutSeconds)
    do {
        Start-Sleep -Milliseconds 100
        $process.Refresh()
        if ($process.HasExited) { throw "Der Launcher wurde vor der Messung beendet." }
        if ($process.MainWindowHandle -ne 0 -and $process.Responding) {
            $windowReadyMs = [math]::Round($stopwatch.Elapsed.TotalMilliseconds, 2)
            break
        }
    } while ((Get-Date) -lt $deadline)

    if ($null -eq $windowReadyMs) {
        throw "Das Launcher-Fenster wurde innerhalb von $ReadyTimeoutSeconds Sekunden nicht bedienbar."
    }

    Start-Sleep -Seconds $IdleSeconds
    $treeIds = Get-ProcessTreeIds -RootId $process.Id
    $tree = foreach ($id in $treeIds) {
        Get-Process -Id $id -ErrorAction SilentlyContinue
    }
    $workingSet = ($tree | Measure-Object WorkingSet64 -Sum).Sum
    $privateMemory = ($tree | Measure-Object PrivateMemorySize64 -Sum).Sum

    $result = [ordered]@{
        measuredAt = (Get-Date).ToString("o")
        executable = $resolved
        windows = $os.Caption
        windowsVersion = $os.Version
        cpu = $cpu.Name
        logicalProcessors = $computer.NumberOfLogicalProcessors
        memoryGiB = [math]::Round($computer.TotalPhysicalMemory / 1GB, 2)
        idleSeconds = $IdleSeconds
        windowReadyProxyMs = $windowReadyMs
        processCount = @($tree).Count
        processTreeWorkingSetMiB = [math]::Round($workingSet / 1MB, 2)
        processTreePrivateMemoryMiB = [math]::Round($privateMemory / 1MB, 2)
        targets = [ordered]@{
            coldStartToShellReadyMs = 3000
            idleProcessTreeWorkingSetMiB = 220
        }
        interpretation = [ordered]@{
            startMetric = "Zeit bis ein sichtbares, reagierendes Hauptfenster vorhanden ist; dies ist ein reproduzierbarer Windows-Proxy und nicht identisch mit der internen s9lab.shell.ready-Markierung."
            memoryMetric = "Summe des Launcher- und aller erkannten Kindprozesse einschließlich WebView2."
            publication = "Der gemessene lokale Build ist unsigniert und nicht zur Verteilung freigegeben."
        }
    }

    $result | ConvertTo-Json -Depth 6 | Set-Content -Path $Output -Encoding utf8
    $result | Format-List
}
finally {
    $treeIds = if ($process) { Get-ProcessTreeIds -RootId $process.Id } else { @() }
    foreach ($id in ($treeIds | Sort-Object -Descending)) {
        Stop-Process -Id $id -Force -ErrorAction SilentlyContinue
    }
}
