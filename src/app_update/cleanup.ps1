$ErrorActionPreference = 'Stop'
$directory = $env:MINT_UPDATE_DIRECTORY
$parentId = [int]$env:MINT_UPDATE_PARENT
$previous = [IO.Path]::Combine($directory, 'previous.exe')
$committed = [IO.Path]::Combine($directory, 'committed')

try {
    $parent = [Diagnostics.Process]::GetProcessById($parentId)
    [IO.File]::WriteAllText([IO.Path]::Combine($directory, 'ready'), '')
    $parent.WaitForExit()
    $parent.Dispose()

    # An interrupted replacement must retain its recovery copy.
    if (-not [IO.File]::Exists($committed)) { exit 0 }

    for ($attempt = 0; $attempt -lt 120; $attempt++) {
        try {
            [IO.File]::Delete($previous)
            [IO.File]::Delete($committed)
            [IO.Directory]::Delete($directory)
            exit 0
        } catch {
            if ($attempt -eq 119) { throw }
            Start-Sleep -Milliseconds 500
        }
    }
} catch {
    # Keep a local diagnostic if permissions or a persistent lock prevent cleanup.
    if ([IO.Directory]::Exists($directory)) {
        [IO.File]::WriteAllText([IO.Path]::Combine($directory, 'cleanup-error.txt'), $_.Exception.Message)
    }
    exit 1
}
