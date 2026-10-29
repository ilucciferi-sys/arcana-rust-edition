param([ValidateSet('performance','high','cinematic')][string]$Quality='high',[string]$ServerAddress)
$ErrorActionPreference='Stop'
$forkPath=Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$runPath=Join-Path $forkPath ('.local\arcana-runs\'+[guid]::NewGuid().ToString('N'))
$binPath=Join-Path $runPath 'bin'
New-Item -ItemType Directory -Path $binPath -Force | Out-Null
foreach($name in @('horizon-client.exe','horizon-ctl.exe')) {
    Copy-Item -LiteralPath (Join-Path $forkPath ('target\fast\'+$name)) -Destination $binPath
}
foreach($entry in @{
    HORIZON_HOME='game';HORIZON_LAUNCHER_HOME='launcher';XDG_DATA_HOME='data';XDG_CONFIG_HOME='config';
    HORIZON_LAUNCHER_TRASH='trash';HORIZON_LAUNCHER_CLIPBOARD='clipboard';HORIZON_LAUNCHER_DESKTOP='desktop'
}.GetEnumerator()) {
    $directory=Join-Path $runPath $entry.Value
    New-Item -ItemType Directory -Path $directory -Force | Out-Null
    Set-Item -LiteralPath ('Env:'+$entry.Key) -Value $directory
}
$env:HORIZON_LAUNCHER_NO_SELF_UPDATE='1'
$env:HORIZON_NO_AUDIO='1'
$env:HORIZON_NO_DISCORD='1'
$env:ARCANA_QUALITY=$Quality
$endpoint=Join-Path $runPath 'control.json'
$resources=Join-Path $forkPath '.local\horizon-client\26.3'
$arguments=@('--resources',('"'+$resources+'"'),'--world','ArcanaLab','--creative','--test-terrain',
    '--render-distance','4','--width','1280','--height','720','--offscreen','--control','--control-socket',('"'+$endpoint+'"'))
if($ServerAddress){$arguments+=@('--server',$ServerAddress,'--name','ArcanaWitness')}
$process=Start-Process -FilePath (Join-Path $binPath 'horizon-client.exe') -ArgumentList $arguments -WorkingDirectory $runPath -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $runPath 'stdout.log') -RedirectStandardError (Join-Path $runPath 'stderr.log')
Set-Content -LiteralPath (Join-Path $forkPath '.local\arcana-latest-run.txt') -Value $runPath
@{run=$runPath;process_id=$process.Id;quality=$Quality} | ConvertTo-Json -Compress
