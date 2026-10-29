# Guest ritual, occupied-circle rejection, disconnect cleanup, then host awakening.
$ErrorActionPreference='Stop'
$hostRun=$null;$observerRun=$null
function Invoke-Client([string]$Root,[string[]]$Words) {
    $raw=& (Join-Path $Root 'bin\horizon-ctl.exe') --socket (Join-Path $Root 'control.json') --timeout 30 @Words
    if($LASTEXITCODE){throw ($raw -join "`n")}
    $raw | ConvertFrom-Json
}
function Ready([string]$Root) {
    $until=[DateTime]::UtcNow.AddSeconds(25)
    while(-not(Test-Path -LiteralPath (Join-Path $Root 'control.json')) -and [DateTime]::UtcNow -lt $until){Start-Sleep -Milliseconds 200}
    Invoke-Client $Root @('wait-ready') | Out-Null
}
function Wait-Magic([string]$Root,[scriptblock]$Condition) {
    $until=[DateTime]::UtcNow.AddSeconds(12)
    do{
        $s=Invoke-Client $Root @('arcana-status')
        if(& $Condition $s){return $s}
        Start-Sleep -Milliseconds 100
    }while([DateTime]::UtcNow -lt $until)
    $s | ConvertTo-Json -Depth 15 | Set-Content -LiteralPath (Join-Path $Root 'failed-ritual-state.json')
    throw 'Expected network ritual state was not reached'
}
function Command([string]$Text){Invoke-Client $hostRun @('command',$Text) | Out-Null}
try {
    $hostRun=(& (Join-Path $PSScriptRoot 'start-validation.ps1') | ConvertFrom-Json).run
    Ready $hostRun
    foreach($cmd in @('gamerule minecraft:spawn_mobs false','fill -6 74 -6 6 74 6 minecraft:stone','fill -6 75 -6 6 85 6 minecraft:air','time set 1000','setblock 0 74 0 minecraft:obsidian')){Command $cmd}
    foreach($distance in @(1,2)){
        $name=if($distance -eq 1){'obsidian'}else{'magma_block'}
        foreach($pair in @(@($distance,0),@(-$distance,0),@(0,$distance),@(0,-$distance))){Command ('setblock '+$pair[0]+' 74 '+$pair[1]+' minecraft:'+$name)}
    }
    foreach($x in @(-2,2)){foreach($z in @(-2,2)){Command "setblock $x 74 $z minecraft:obsidian";Command "setblock $x 75 $z minecraft:campfire[lit=true]"}}
    Command 'setblock 3 74 0 minecraft:lava[level=0]'
    Command 'tp @s 0.5 75 0.5'
    Command 'kill @e[type=minecraft:item]'
    Command 'clear @s'
    $port=(Invoke-Client $hostRun @('lan','open','0')).port
    $observerRun=(& (Join-Path $PSScriptRoot 'start-validation.ps1') -ServerAddress ('127.0.0.1:'+$port) | ConvertFrom-Json).run
    Ready $observerRun
    Command 'tp ArcanaWitness 0.5 75 0.5'
    Start-Sleep -Milliseconds 500
    Invoke-Client $hostRun @('key','f5','tap') | Out-Null
    Invoke-Client $observerRun @('key','shift','down') | Out-Null
    $guestActive=Wait-Magic $observerRun {param($s) $null -ne $s.snapshot.ritual}
    Invoke-Client $observerRun @('key','shift','up') | Out-Null
    Invoke-Client $hostRun @('key','shift','down') | Out-Null
    $busy=Wait-Magic $hostRun {param($s) $s.snapshot.notice -like '*занято*'}
    if($busy.snapshot.ritual -or $busy.snapshot.state.school -or $busy.ritual_visuals -ne 1){throw 'Occupied-circle or observer visual state is incorrect'}
    Invoke-Client $hostRun @('screenshot',(Join-Path $hostRun 'guest-ritual.png')) | Out-Null
    Invoke-Client $hostRun @('key','shift','up') | Out-Null
    Invoke-Client $observerRun @('quit') | Out-Null
    $cleared=Wait-Magic $hostRun {param($s) $s.ritual_visuals -eq 0}
    Invoke-Client $hostRun @('key','shift','down') | Out-Null
    $restarted=Wait-Magic $hostRun {param($s) $null -ne $s.snapshot.ritual}
    Invoke-Client $hostRun @('key','shift','up') | Out-Null
    $bound=Wait-Magic $hostRun {param($s) $s.snapshot.state.school -eq 'fire'}
    if($bound.snapshot.state.xp -ne 0){throw 'Host received unexpected progression'}
    @{host_run=$hostRun;guest_run=$observerRun;guest_active=$guestActive;busy=$busy;cleared=$cleared;restarted=$restarted;bound=$bound;visual_review='required'} | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath (Join-Path $hostRun 'ritual-network-report.json')
    @{host_run=$hostRun;guest_run=$observerRun;occupied='passed';disconnect_cleanup='passed';host_awakening='passed'} | ConvertTo-Json -Compress
}finally{
    foreach($run in @($observerRun,$hostRun)){
        if($run -and (Test-Path -LiteralPath (Join-Path $run 'control.json'))){try{Invoke-Client $run @('quit') | Out-Null}catch{Write-Warning $_}}
    }
}
