# Two fresh native clients: owner-only progress and shared projectile visuals.
$ErrorActionPreference='Stop'
$forkPath=Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$hostRun=$null
$observerRun=$null
function Invoke-Client([string]$Root,[string[]]$Words) {
    $reply=& (Join-Path $Root 'bin\horizon-ctl.exe') --socket (Join-Path $Root 'control.json') --timeout 30 @Words
    if($LASTEXITCODE){throw ($reply -join "`n")}
    $reply | ConvertFrom-Json
}
function Wait-Client([string]$Root) {
    $deadline=[DateTime]::UtcNow.AddSeconds(25)
    while(-not(Test-Path -LiteralPath (Join-Path $Root 'control.json')) -and [DateTime]::UtcNow -lt $deadline){Start-Sleep -Milliseconds 250}
    Invoke-Client $Root @('wait-ready') | Out-Null
}
try {
    $hostRun=(& (Join-Path $PSScriptRoot 'start-validation.ps1') | ConvertFrom-Json).run
    Wait-Client $hostRun
    foreach($command in @('gamerule minecraft:spawn_mobs false','fill -12 74 -16 12 74 20 minecraft:stone','fill -12 75 -16 12 84 20 minecraft:air','time set 1000','summon minecraft:iron_golem 0 75 10 {NoAI:1b,PersistenceRequired:1b}')) {
        Invoke-Client $hostRun @('command',$command) | Out-Null
    }
    Invoke-Client $hostRun @('tp','0.5','75','-8') | Out-Null
    Invoke-Client $hostRun @('look','0','0') | Out-Null
    Invoke-Client $hostRun @('arcana-demo') | Out-Null
    $port=(Invoke-Client $hostRun @('lan','open','0')).port
    $observerRun=(& (Join-Path $PSScriptRoot 'start-validation.ps1') -ServerAddress ('127.0.0.1:'+$port) | ConvertFrom-Json).run
    Wait-Client $observerRun
    Invoke-Client $hostRun @('command','tp ArcanaWitness 3 75 -6') | Out-Null
    Invoke-Client $observerRun @('look','14','0') | Out-Null
    Start-Sleep -Milliseconds 600
    Invoke-Client $hostRun @('key','r','tap') | Out-Null
    Start-Sleep -Milliseconds 150
    Invoke-Client $hostRun @('key','v','tap') | Out-Null
    Start-Sleep -Milliseconds 370
    Invoke-Client $observerRun @('screenshot',(Join-Path $observerRun 'starter-flight.png')) | Out-Null
    Start-Sleep -Milliseconds 150
    Invoke-Client $observerRun @('screenshot',(Join-Path $observerRun 'starter-flight-later.png')) | Out-Null
    Start-Sleep -Milliseconds 700
    $owner=Invoke-Client $hostRun @('arcana-status')
    $observer=Invoke-Client $observerRun @('arcana-status')
    $lan=Invoke-Client $hostRun @('lan','status')
    $mobs=Invoke-Client $hostRun @('mobs','40')
    if($owner.snapshot.state.xp -le 1600){throw 'Owner did not gain XP'}
    if(-not $observer.snapshot -or $observer.snapshot.state.school){throw 'Owner progress leaked to observer'}
    $releases=@(Select-String -LiteralPath @((Join-Path $hostRun 'stderr.log'),(Join-Path $hostRun 'stdout.log')) -SimpleMatch 'Arcana spark impact: target=Some').Count
    if($releases -ne 1){throw "Expected one impact, received $releases"}
    @{host_run=$hostRun;observer_run=$observerRun;owner=$owner;observer=$observer;lan=$lan;mobs=$mobs;server_impacts=$releases;visual_review='required'} |
        ConvertTo-Json -Depth 20 | Set-Content -LiteralPath (Join-Path $observerRun 'starter-network-report.json')
    @{host_run=$hostRun;observer_run=$observerRun;server_impacts=$releases;result='passed_requires_visual_review'} | ConvertTo-Json -Compress
} finally {
    foreach($run in @($observerRun,$hostRun)) {
        if($run -and (Test-Path -LiteralPath (Join-Path $run 'control.json'))) {
            try{Invoke-Client $run @('quit') | Out-Null}catch{Write-Warning $_}
        }
    }
}
