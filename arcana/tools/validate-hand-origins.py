#!/usr/bin/env python3
"""Compare authoritative palms with actual native models on two isolated clients."""
from pathlib import Path
import argparse
import hashlib
import importlib.util
import json
import math
import shutil
import time
import uuid

spec = importlib.util.spec_from_file_location('fire_school_validation', Path(__file__).with_name('validate-fire-school.py'))
school = importlib.util.module_from_spec(spec)
spec.loader.exec_module(school)


def pose(game, spell, own):
    return next((p for p in game.magic()['cast_poses'] if p['spell'] == spell and p['own'] == own), None)


def origin(game, spell):
    return next((p for p in game.magic()['server_fire']['origins'] if p['spell'] == spell), None)


def validate(root, binary, name):
    host = observer = None
    evidence = {'alignment': {}, 'fire_ignites': False}

    def save():
        (root / 'report.json').write_text(json.dumps(evidence, ensure_ascii=False, indent=2), encoding='utf-8')

    def aligned(name, left=False, tolerance=.07):
        def sample():
            server = origin(host, 'inferno_beam')
            own = pose(host, 'inferno_beam', True)
            seen = pose(observer, 'inferno_beam', False)
            if not all([server, own, seen]):
                return None
            errors = [math.dist(server['palm'], p['palm']) for p in [own, seen]]
            return {'server': server, 'owner': own, 'witness': seen, 'errors_metres': errors,
                    'correct_hand': all(p['left'] == left for p in [server,own,seen])}
        result = host.wait(sample, lambda p: p is not None and p['correct_hand'] and max(p['errors_metres']) < tolerance, 4)
        assert not result['server']['clipped'], result
        assert len({result[k]['slim'] for k in ['server','owner','witness']}) == 1, result
        evidence['alignment'][name] = result
        save()
        return result

    try:
        # Isolate projectile/line-of-sight checks from the original blast's
        # ground-fire patch, which spreads around a one-block-thick wall.
        host = school.Game(root / 'host', binary, player_name=name, fire_config={'fire_ignites':False})
        school.arena(host)
        port = host.call('lan open 0 commands')['port']
        observer = school.Game(root / 'observer', binary, f'127.0.0.1:{port}')
        host.command('gamemode spectator ArcanaWitness')
        host.command('tp ArcanaWitness 3.5 76 -2.5')
        observer.wait(lambda: observer.call('status'), lambda s: abs(s['player']['position'][0]-3.5)<.1)
        observer.call('look-at 0.5 76 -6.5')
        host.select('inferno_beam')
        host.call('key v tap')
        host.wait(host.magic, lambda m: m['snapshot']['cast'] is not None and m['snapshot']['cast']['phase'] == 'Channeling')
        for name, yaw, pitch in [('level',0,0), ('up',0,-55), ('down',0,55), ('turned',90,-25)]:
            host.call(f'look {yaw} {pitch}')
            time.sleep(.25)
            aligned(name)
        host.call('look 0 0')
        host.call('options-set mainHand left')
        time.sleep(.25)
        aligned('left_hand', True)
        host.shot('beam-left-hand-first-person')
        observer.shot('beam-left-hand-witness')
        host.call('key shift down')
        time.sleep(.4)
        aligned('crouching', True)
        host.shot('beam-crouching')
        host.call('key shift up')
        host.call('options-set mainHand right')
        host.call('look 0 -10')
        time.sleep(.3)
        start = host.call('status')['player']['position']
        host.call('key w down')
        time.sleep(.45)
        moving = aligned('walking', tolerance=.25)
        end = host.call('status')['player']['position']
        host.call('release')
        assert math.dist(start,end)>.05, (start,end)
        evidence['walking'] = {'start':start,'end':end,'alignment':moving}
        host.call('key x tap')
        host.wait(host.magic, lambda m: m['snapshot']['cast'] is None)
        host.wait(lambda: host.magic()['cast_poses'], lambda p: not p)

        # A wide wall just beyond the player's collision box must intercept
        # the eye-to-palm segment before a forward-reaching release pose.
        host.call('arcana-demo')
        host.select('fireball')
        host.command('tp @s 0.5 75 -6.31')
        host.call('look 0 0')
        host.command('fill -4 75 -6 4 79 -6 minecraft:stone')
        school.summon(host, z=-3.5)
        time.sleep(.3)
        host.call('key v tap')
        release = host.wait(lambda: origin(host,'fireball'), lambda p: p is not None and p['age'] >= 15)
        assert release['clipped'], release
        assert release['start'][2] < -6 and release['palm'][2] > -6, release
        time.sleep(.7)
        evidence['wall'] = {'origin':release,'targets':host.mobs()}
        save()
        assert host.mobs() and all(m['health']==100 for m in host.mobs()), host.mobs()
        host.shot('wall-blocks-release')
        observer.shot('wall-blocks-release-witness')
        evidence['result'] = 'passed'
        save()
    except BaseException:
        evidence['result'] = 'failed'
        save()
        raise
    finally:
        for game in [observer, host]:
            if game:
                game.stop()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,default=school.FORK/'target/fast/horizon-client.exe')
    parser.add_argument('--name',default='ArcanaMage',help='Offline profile name, also determines default skin width')
    args = parser.parse_args()
    root = school.FORK/'.local/hand-origin-validation'/uuid.uuid4().hex
    root.mkdir(parents=True)
    binary = root/args.binary.name
    shutil.copy2(args.binary,binary)
    (root/'binary.sha256').write_text(hashlib.sha256(binary.read_bytes()).hexdigest(),encoding='ascii')
    print(root,flush=True)
    validate(root,binary,args.name)
    print('PASS: native palm alignment, both hands, aim, crouch, movement and wall interception',flush=True)


if __name__ == '__main__':
    main()
