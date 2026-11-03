#!/usr/bin/env python3
"""Two native clients: Fire vortex, meteor wake/contact, late join and cleanup."""
from pathlib import Path
import argparse
import hashlib
import importlib.util
import json
import os
import shutil
import time
import uuid

spec = importlib.util.spec_from_file_location('school', Path(__file__).with_name('validate-fire-school.py'))
school = importlib.util.module_from_spec(spec)
spec.loader.exec_module(school)


def effects(game, stage):
    return [v for v in game.magic()['tempests'] if v['stage'] == stage]


def validate(root, binary):
    host = observer = None
    report = {'result': 'running', 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
              'quality': os.environ.get('ARCANA_QUALITY', 'high'),
              'reduced': bool(os.environ.get('ARCANA_REDUCED_EFFECTS'))}

    def save():
        (root / 'report.json').write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding='utf-8')

    def check(scene):
        assert scene['marks'] > 0 and scene['marks'] <= scene['budget'], scene
        assert scene['planes'] > 0, scene
        assert scene['reduced'] == report['reduced'], scene
        return scene

    def frame_budget(game):
        values = game.magic()['tempests']
        assert sum(v['marks'] for v in values) <= 8192, values
        return values

    try:
        host = school.Game(root / 'host', binary, fire_config={'fire_ignites': False, 'spell_griefing': False})
        school.arena(host)
        host.command('weather clear')
        port = host.call('lan open 0 commands')['port']
        observer = school.Game(root / 'observer', binary, f'127.0.0.1:{port}')
        host.command('gamemode spectator ArcanaWitness')
        host.command('tp ArcanaWitness 13.5 81 -20.5')
        observer.wait(lambda: observer.call('status'), lambda s: abs(s['player']['position'][0] - 13.5) < .1)
        observer.call('look-at 0.5 80 -3.5')
        school.summon(host, z=-3.5, physics=True)
        host.select('fire_storm')
        before = host.state()
        host.call('key v tap')
        own = check(host.wait(lambda: effects(host, 'vortex'), bool)[0])
        seen = check(observer.wait(lambda: effects(observer, 'vortex'), bool)[0])
        host.wait(host.magic, lambda s: s['snapshot']['state']['xp'] > before['xp'])
        host.wait(lambda: effects(host, 'vortex'), lambda values: bool(values) and values[0]['age_seconds'] > 2)
        host.shot('storm-first-person')
        observer.shot('storm-witness')
        report['storm'] = {'owner': own, 'witness': seen}
        save()

        observer.call('disconnect')
        observer.call(f'connect 127.0.0.1:{port} ArcanaWitness')
        late = check(observer.wait(lambda: effects(observer, 'vortex'), bool, 15)[0])
        assert late['id'] == seen['id'], (late, seen)
        report['late_join'] = late
        # View-dependent LOD and range culling, while the server still simulates it.
        field = next(v['effect'] for v in host.magic()['fire_fields'] if v['effect']['spell_id'] == 'fire_storm')
        x, y, z = field['position']
        host.command(f'tp ArcanaWitness {x + 85} {y + 7} {z - 3}')
        observer.wait(lambda: observer.call('status'), lambda s: abs(s['player']['position'][0] - x - 85) < .2)
        distant = check(observer.wait(lambda: effects(observer, 'vortex'), lambda s: bool(s) and s[0]['distance'] > 70)[0])
        assert distant['budget'] == 192, distant
        report['distant_lod'] = distant
        host.command(f'tp ArcanaWitness {x + 210} {y + 7} {z}')
        observer.wait(lambda: observer.call('status'), lambda s: s['player']['position'][0] > x + 200)
        observer.wait(lambda: effects(observer, 'vortex'), lambda s: not s)
        report['range_cull'] = True
        # Restore the camera and wait for the original thirty-second effect.
        host.command('tp ArcanaWitness 13.5 81 -20.5')
        observer.wait(lambda: observer.call('status'), lambda s: abs(s['player']['position'][0] - 13.5) < .1)
        observer.call('look-at 0.5 81 -2.5')
        host.wait(lambda: effects(host, 'vortex'), lambda s: not s, 35)
        observer.wait(lambda: effects(observer, 'vortex'), lambda s: not s)
        report['storm_natural_expiry'] = True

        host.call('arcana-demo')
        host.select('meteor_rain')
        meteor_xp_before = host.state()['xp']
        host.call('key v tap')
        canopy = check(observer.wait(lambda: effects(observer, 'canopy'), bool)[0])
        observer.call('look-at 0.5 89 -6.5')
        observer.shot('meteor-sky-seal')
        meteor = host.wait(host.magic, lambda s: any(b['kind'] == 'Meteor' for b in s['server_fire']['projectile_states']))
        bolt = next(b for b in meteor['server_fire']['projectile_states'] if b['kind'] == 'Meteor')
        x, y, z = bolt['position']
        host.command(f'tp ArcanaWitness {x + 8} {y - 2} {z - 10}')
        observer.wait(lambda: observer.call('status'), lambda s: abs(s['player']['position'][0] - x - 8) < .2)
        observer.call(f'look-at {x} {y - 4} {z}')
        flight = check(observer.wait(lambda: effects(observer, 'flight'), bool)[0])
        assert flight['rocks'] > 0, flight
        observer.shot('meteor-flight')
        # Choose a fresh, real server meteor after taking the flight frame. The
        # photographed rock may already have landed while screenshot IO ran.
        strike = host.wait(host.magic, lambda s: any(b['kind'] == 'Meteor' and b['age'] <= 2 for b in s['server_fire']['projectile_states']))
        bolt = next(b for b in strike['server_fire']['projectile_states'] if b['kind'] == 'Meteor' and b['age'] <= 2)
        x, y, z = bolt['position']
        host.command(f'fill {int(x)-2} {int(y)-9} {int(z)-2} {int(x)+2} {int(y)-9} {int(z)+2} minecraft:stone')
        school.summon(host, x=x, y=y - 8, z=z)
        host.command(f'tp ArcanaWitness {x + 10} {y - 4} {z - 12}')
        observer.wait(lambda: observer.call('status'), lambda s: abs(s['player']['position'][0] - x - 10) < .2)
        observer.call(f'look-at {x} {y - 6} {z}')
        contacts = observer.wait(lambda: [v for v in effects(observer, 'impact') if .12 < v['age_seconds'] < .7 and (v['position'][0]-x)**2 + (v['position'][2]-z)**2 < 9], bool, 8)
        contact = check(min(contacts, key=lambda v: (v['position'][0]-x)**2 + (v['position'][2]-z)**2))
        cx, cy, cz = contact['position']
        observer.call(f'look-at {cx} {cy + .7} {cz}')
        observer.shot('meteor-impact')
        rewarded = host.wait(host.magic, lambda s: s['snapshot']['state']['xp'] > meteor_xp_before)
        report['meteor'] = {'canopy': canopy, 'flight': flight, 'contact': contact,
                            'xp_before': meteor_xp_before, 'xp_after': rewarded['snapshot']['state']['xp'],
                            'frame': frame_budget(observer)}
        # A second contact on bare stone exposes the actual burst composition;
        # vanilla fire on the damaged golem can obscure the first close-up.
        host.command('kill @e[type=minecraft:iron_golem]')
        strike = host.wait(host.magic, lambda s: any(b['kind'] == 'Meteor' and b['age'] <= 2 for b in s['server_fire']['projectile_states']))
        bolt = next(b for b in strike['server_fire']['projectile_states'] if b['kind'] == 'Meteor' and b['age'] <= 2)
        x, y, z = bolt['position']
        host.command(f'fill {int(x)-3} {int(y)-8} {int(z)-3} {int(x)+3} {int(y)-8} {int(z)+3} minecraft:stone')
        host.command(f'tp ArcanaWitness {x + 9} {y - 3} {z - 11}')
        observer.wait(lambda: observer.call('status'), lambda s: abs(s['player']['position'][0] - x - 9) < .2)
        observer.call(f'look-at {x} {int(y)-6} {z}')
        surface = observer.wait(lambda: [v for v in effects(observer, 'impact') if v['id'] == bolt['id'] and .18 < v['age_seconds'] < .6], bool, 8)[0]
        report['meteor']['surface_contact'] = check(surface)
        observer.shot('meteor-surface-impact')
        save()
        # Late join receives ongoing rain/projectiles, without replaying contacts.
        observer.call('disconnect')
        observer.call(f'connect 127.0.0.1:{port} ArcanaWitness')
        report['meteor_late_join'] = check(observer.wait(lambda: effects(observer, 'canopy'), bool, 15)[0])
        host.wait(lambda: effects(host, 'canopy'), lambda s: not s, 22)
        host.wait(lambda: host.magic()['tempests'], lambda s: not s, 8)
        observer.wait(lambda: observer.magic()['tempests'], lambda s: not s, 8)
        report['meteor_natural_expiry'] = True

        # Death during an active field must clear owner and witness immediately.
        host.call('arcana-demo')
        host.select('fire_storm')
        host.call('key v tap')
        observer.wait(lambda: effects(observer, 'vortex'), bool)
        host.command('kill @s')
        host.wait(lambda: host.magic()['tempests'], lambda s: not s)
        observer.wait(lambda: observer.magic()['tempests'], lambda s: not s)
        host.call('respawn')
        host.wait(host.magic, lambda s: s['snapshot'] is not None and not s['tempests'])
        report['death_respawn_cleanup'] = True
        report['result'] = 'passed'
        save()
        print(json.dumps({'result': 'passed', 'run': str(root), 'quality': report['quality'], 'reduced': report['reduced']}), flush=True)
    except BaseException as error:
        report['result'], report['failure'] = 'failed', str(error)
        save()
        raise
    finally:
        for game in [observer, host]:
            if game:
                game.stop()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--quality', choices=['performance', 'high', 'cinematic'], default='high')
    parser.add_argument('--reduced', action='store_true')
    args = parser.parse_args()
    os.environ['ARCANA_QUALITY'] = args.quality
    if args.reduced:
        os.environ['ARCANA_REDUCED_EFFECTS'] = '1'
    else:
        os.environ.pop('ARCANA_REDUCED_EFFECTS', None)
    root = school.FORK / '.local/fire-tempest-validation' / uuid.uuid4().hex
    binary = root / 'horizon-client.exe'
    root.mkdir(parents=True)
    shutil.copy2(school.FORK / 'target/fast/horizon-client.exe', binary)
    print(json.dumps({'run': str(root)}), flush=True)
    validate(root, binary)


if __name__ == '__main__':
    main()
