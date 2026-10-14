param([int]$Repeats = 3)
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $repoRoot '.task40-target/debug/aether.exe'
$results = @()
$caseIndex = 0
foreach ($language in @('ru', 'en')) {
    for ($iteration = 1; $iteration -le $Repeats; $iteration++) {
        $port = 43980 + $caseIndex
        $caseIndex++
        $log = Join-Path $repoRoot ".task40-build/detail-$language-$iteration.log"
        $stdout = Join-Path $repoRoot ".task40-build/detail-$language-$iteration.stdout.log"
        $owned = Start-Process -FilePath $exe -ArgumentList '--service', '--port', $port, '--token', 'audit-local-only', '--no-overlay', '--detail-diagnostic', '--slow-snapshot-diagnostic', '--lang', $language -WindowStyle Hidden -PassThru -RedirectStandardError $log -RedirectStandardOutput $stdout
        if (!$owned.WaitForExit(15000)) {
            $owned.Refresh()
            if (!$owned.HasExited -and $owned.Path -eq $exe) { Stop-Process -Id $owned.Id }
            throw "Owned diagnostic timed out: $language $iteration; $log"
        }
        $line = Get-Content $log | Where-Object { $_.StartsWith('[dps-detail] renderer diagnostic: ') } | Select-Object -Last 1
        if (!$line) { throw "Missing renderer report: $log" }
        $report = $line.Substring('[dps-detail] renderer diagnostic: '.Length) | ConvertFrom-Json
        foreach ($field in @('rendered','liveRendered','unchangedDomReused','changedDomUpdated','scrollPreserved','buttonClosed','escapeClosed','closedWhileSnapshotPending')) {
            if ($report.$field -ne $true) { throw "Diagnostic $field failed: $log" }
        }
        if ($report.repeatedCloseCycles -ne 10 -or $report.scrollPixels -le 0 -or $report.initialRenderMs -ge 500 -or $report.closeDuringSnapshotMs -ge 500 -or $owned.ExitCode -ne 0) { throw "Diagnostic lifecycle/render budget failed: $log" }
        $results += [pscustomobject]@{Language=$language;Iteration=$iteration;PID=$owned.Id;InitialRenderMs=$report.initialRenderMs;CloseDuringSnapshotMs=$report.closeDuringSnapshotMs;HeartbeatMs=$report.heartbeatMs;CloseCycles=$report.repeatedCloseCycles;Status='PASS';Log=$log}
        $results | ConvertTo-Json | Set-Content (Join-Path $repoRoot '.task40-build/detail-native-results.json')
        Write-Output "$language/$iteration PASS; render=$($report.initialRenderMs)ms; 10 close cycles"
    }
}
