# Two fresh native clients, real keyboard actions, server-owned combat assertions.
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
function Command([string]$Text){Invoke-Client $hostRun @('command',$Text) | Out-Null}
function Magic { (Invoke-Client $hostRun @('arcana-status')).snapshot }
function Wait-Magic([scriptblock]$Condition) {
    $until=[DateTime]::UtcNow.AddSeconds(8)
    do{$s=Magic;if(& $Condition $s){return $s};Start-Sleep -Milliseconds 75}while([DateTime]::UtcNow -lt $until)
    throw 'Expected magic state not reached'
}
function Select-Spell([string]$Id) {
    for($i=0;$i -lt 12;$i++){
        if((Magic).state.selected -eq $Id){return}
        Invoke-Client $hostRun @('key','r','tap') | Out-Null;Start-Sleep -Milliseconds 100
    }
    throw "Could not select $Id"
}
function Arena {
    Command 'kill @e[type=minecraft:iron_golem]'
    Invoke-Client $hostRun @('wait','21') | Out-Null
    Command 'fill -20 74 -12 20 74 20 minecraft:stone'
    Command 'fill -20 75 -12 20 85 20 minecraft:air'
    Command 'fill 2 75 0 6 79 0 minecraft:stone'
    foreach($target in @(@('near',0.5,2.5),@('far',0.5,7.5),@('blocked',4.5,2.5),@('behind',0.5,-8.5),@('side',-6.5,-1.5),@('outside',0.5,12.6))){
        Command ('summon minecraft:iron_golem '+$target[1]+' 75 '+$target[2]+' {NoAI:1b,PersistenceRequired:1b,Tags:["'+$target[0]+'"]}')
    }
    # NoAI in Horizon skips travel; use active physics with zero walking speed
    # for the target whose displacement is asserted.
    Command 'data merge entity @e[tag=near,limit=1] {NoAI:0b}'
    Command 'attribute @e[tag=near,limit=1] minecraft:movement_speed base set 0'
    Command 'tp @s 0.5 75 -3.5'
    Start-Sleep -Milliseconds 200
    $mobs=Invoke-Client $hostRun @('mobs','40')
    $script:ids=@{}
    foreach($target in @(@('near',0.5,2.5),@('far',0.5,7.5),@('blocked',4.5,2.5),@('behind',0.5,-8.5),@('side',-6.5,-1.5),@('outside',0.5,12.6))){
        $m=$mobs.near | Where-Object {$_.kind -eq 'minecraft:iron_golem' -and $_.health -gt 0 -and [Math]::Abs($_.pos[0]-$target[1]) -lt 0.2 -and [Math]::Abs($_.pos[2]-$target[2]) -lt 0.2} | Select-Object -First 1
        if(-not $m){throw ('Missing target '+$target[0])}
        $script:ids[$target[0]]=$m.id
    }
}
function Targets {
    $mobs=Invoke-Client $hostRun @('mobs','40');$targets=@{}
    foreach($name in $script:ids.Keys){$targets[$name]=$mobs.near | Where-Object {$_.id -eq $script:ids[$name]} | Select-Object -First 1}
    return $targets
}
try {
    $hostRun=(& (Join-Path $PSScriptRoot 'start-validation.ps1') | ConvertFrom-Json).run;Ready $hostRun
    Command 'gamerule minecraft:spawn_mobs false';Command 'time set 1000'
    Invoke-Client $hostRun @('arcana-demo') | Out-Null
    Wait-Magic {param($s) $s.state.school -eq 'fire'} | Out-Null
    Arena
    Invoke-Client $hostRun @('look','0','0') | Out-Null
    $port=(Invoke-Client $hostRun @('lan','open','0')).port
    $observerRun=(& (Join-Path $PSScriptRoot 'start-validation.ps1') -ServerAddress ('127.0.0.1:'+$port) | ConvertFrom-Json).run;Ready $observerRun
    Command 'tp ArcanaWitness -3.5 75 -5.5'
    Invoke-Client $observerRun @('look','-26','10') | Out-Null
    Invoke-Client $hostRun @('key','f5','tap') | Out-Null
    Select-Spell 'ignition'
    $before=Magic
    Invoke-Client $hostRun @('key','v','tap') | Out-Null;Start-Sleep -Milliseconds 650
    Invoke-Client $observerRun @('screenshot',(Join-Path $observerRun 'ignition.png')) | Out-Null
    Invoke-Client $hostRun @('screenshot',(Join-Path $hostRun 'ignition.png')) | Out-Null
    $ignition=Wait-Magic {param($s) $s.state.xp -eq $before.state.xp+4}
    $ignited=Targets
    if($ignited.near.health -ge 100 -or -not $ignited.near.burning -or $ignited.far.health -ne 100){throw 'Ignition did not choose only the near target'}
    foreach($name in @('blocked','behind','side','outside')){if($ignited[$name].health -ne 100 -or $ignited[$name].burning){throw "Ignition affected $name"}}

    Invoke-Client $hostRun @('arcana-demo') | Out-Null
    Arena;Select-Spell 'fire_push'
    $before=Magic
    Invoke-Client $hostRun @('key','v','tap') | Out-Null;Start-Sleep -Milliseconds 650
    Invoke-Client $observerRun @('screenshot',(Join-Path $observerRun 'fire-push.png')) | Out-Null
    Invoke-Client $hostRun @('screenshot',(Join-Path $hostRun 'fire-push.png')) | Out-Null
    $push=Wait-Magic {param($s) $s.state.xp -eq $before.state.xp+4}
    $pushed=Targets
    foreach($name in @('near','far')){if($pushed[$name].health -ge 100 -or -not $pushed[$name].burning){throw "Push did not burn/damage $name"}}
    foreach($name in @('blocked','behind','side','outside')){if($pushed[$name].health -ne 100 -or $pushed[$name].burning){throw "Push affected $name"}}
    if($pushed.near.pos[2] -le 2.6){throw 'Push did not move the target'}

    # A fresh hurt cooldown rejects the three damage, but not the raw impulse.
    Invoke-Client $hostRun @('arcana-demo') | Out-Null
    Command 'kill @e[type=minecraft:iron_golem]'
    Command 'summon minecraft:iron_golem 0.5 75 2.5 {PersistenceRequired:1b,Tags:["immune"]}'
    Command 'attribute @e[tag=immune,limit=1] minecraft:movement_speed base set 0'
    $before=Magic
    Invoke-Client $hostRun @('key','v','tap') | Out-Null;Start-Sleep -Milliseconds 150
    Invoke-Client $hostRun @('server-hold','on') | Out-Null
    Command 'damage @e[tag=immune,limit=1] 10 minecraft:generic'
    Invoke-Client $hostRun @('server-hold','off') | Out-Null
    $immune=Wait-Magic {param($s) $s.state.xp -eq $before.state.xp+4}
    $logs=Get-Content -LiteralPath @((Join-Path $hostRun 'stderr.log'),(Join-Path $hostRun 'stdout.log')) -Raw
    if(($logs -join "`n") -notmatch 'Arcana fire_push: affected=1, damaged=0, pushed=1, xp=4'){throw 'Rejected damage changed push/reward behavior'}

    Invoke-Client $hostRun @('arcana-demo') | Out-Null
    Arena;Select-Spell 'ignition'
    Command 'fill -1 75 0 1 80 0 minecraft:stone'
    $before=Magic
    Invoke-Client $hostRun @('key','v','tap') | Out-Null;Start-Sleep -Milliseconds 800
    $wall=Magic;$blocked=Targets
    if($wall.state.xp -ne $before.state.xp -or $blocked.near.health -ne 100 -or $blocked.near.burning){throw 'Ignition crossed the wall or awarded XP for refused terrain'}
    if($wall.notice -notlike '*Поджигание блоков*'){throw 'Missing terrain limitation notice'}
    $observer=Invoke-Client $observerRun @('arcana-status')
    if($observer.snapshot.state.school -or $observer.snapshot.state.xp -ne 0){throw 'Owner magic state leaked to observer'}
    @{host_run=$hostRun;observer_run=$observerRun;ignition=$ignition;ignited_targets=$ignited;push=$push;pushed_targets=$pushed;rejected_damage=$immune;wall=$wall;observer=$observer;visual_review='required'} | ConvertTo-Json -Depth 22 | Set-Content -LiteralPath (Join-Path $hostRun 'fire-report.json')
    @{host_run=$hostRun;observer_run=$observerRun;combat='passed';line_of_sight='passed';reward='passed';owner_only='passed'} | ConvertTo-Json -Compress
}finally{
    if($hostRun){try{Invoke-Client $hostRun @('server-hold','off') | Out-Null}catch{Write-Warning $_}}
    foreach($run in @($observerRun,$hostRun)){if($run -and (Test-Path -LiteralPath (Join-Path $run 'control.json'))){try{Invoke-Client $run @('quit') | Out-Null}catch{Write-Warning $_}}}
}
