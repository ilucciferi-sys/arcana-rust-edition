#!/usr/bin/env python3
"""Exercise every Fire spell with two real Rust clients and isolated saves.

Requires a previously built target/fast/horizon-client and prepared 26.3 data.
Only Python's standard library is used; players run the Rust binaries directly.
"""
from pathlib import Path
import argparse
import hashlib
import json
import os
import shutil
import socket
import subprocess
import time
import uuid

FORK = Path(__file__).resolve().parents[2]
SPELLS = ['spark', 'ignition', 'fire_push', 'fireball', 'flame_wall', 'flame_shield',
          'fire_dash', 'fire_ring', 'fire_blast', 'inferno_beam', 'fire_storm',
          'phoenix_form', 'meteor_rain', 'solar_spear', 'fire_avatar']
NO_WINDOW = 0x08000000 if os.name == 'nt' else 0


class Game:
    def __init__(self, root, binary, server=None, resume=False, fire_config=None):
        self.root, self.binary = root, binary
        root.mkdir(parents=True, exist_ok=resume)
        env = os.environ.copy()
        for key, name in {'HORIZON_HOME': 'game', 'HORIZON_LAUNCHER_HOME': 'launcher',
                          'XDG_DATA_HOME': 'data', 'XDG_CONFIG_HOME': 'config',
                          'HORIZON_LAUNCHER_TRASH': 'trash', 'HORIZON_LAUNCHER_CLIPBOARD': 'clipboard',
                          'HORIZON_LAUNCHER_DESKTOP': 'desktop'}.items():
            directory = root / name
            directory.mkdir(exist_ok=resume)
            env[key] = str(directory)
        env.update(HORIZON_LAUNCHER_NO_SELF_UPDATE='1', HORIZON_NO_AUDIO='1',
                   HORIZON_NO_DISCORD='1', ARCANA_QUALITY=os.environ.get('ARCANA_QUALITY', 'high'))
        env.pop('ARCANA_DEMO_SCHOOL', None)
        if not server and not resume:
            env['ARCANA_DEMO_SCHOOL'] = 'fire'
        if fire_config is not None:
            config = root / 'saves/ArcanaFireSchool/arcana/fire-config.json'
            config.parent.mkdir(parents=True, exist_ok=True)
            config.write_text(json.dumps(fire_config), encoding='utf-8')
        (root / 'options.txt').write_text('tutorialStep:none\nchatVisibility:1\nguiScale:2\ngamma:0.75\nmaxFps:60\n', encoding='utf-8')
        self.endpoint = root / 'control.json'
        if resume:
            self.endpoint.unlink(missing_ok=True)
        args = [str(binary), '--resources', str(FORK / '.local/horizon-client/26.3'),
                '--world', 'ArcanaFireSchool', '--creative', '--test-terrain',
                '--render-distance', '6', '--width', '1280', '--height', '720',
                '--offscreen', '--control', '--control-socket', str(self.endpoint),
                '--name', 'ArcanaWitness' if server else 'ArcanaMage']
        if server:
            args += ['--server', server]
        self.log = (root / 'session.log').open('ab' if resume else 'wb')
        self.process = subprocess.Popen(args, cwd=root, env=env, stdout=self.log, stderr=self.log, creationflags=NO_WINDOW)
        self.connection = self.stream = None
        try:
            until = time.monotonic() + 45
            while not self.endpoint.exists():
                if self.process.poll() is not None or time.monotonic() > until:
                    raise RuntimeError('Native client failed to start; see session.log')
                time.sleep(.1)
            endpoint = json.loads(self.endpoint.read_text(encoding='utf-8'))
            self.connection = socket.create_connection(('127.0.0.1', endpoint['port']), timeout=35)
            self.connection.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
            self.stream = self.connection.makefile('rwb', buffering=0)
            self.call('auth ' + endpoint['token'])
            self.wait(lambda: self.call('status'), lambda s: s.get('player') is not None and s.get('pending_chunks') == 0 and s.get('server', {}).get('loaded') is not False, 35)
            self.wait(self.magic, lambda s: s.get('snapshot') is not None)
            self.call('release')
        except BaseException:
            self.stop()
            raise

    def call(self, line):
        self.stream.write((line + '\n').encode('utf-8'))
        result = json.loads(self.stream.readline())
        if isinstance(result, dict) and 'error' in result:
            raise RuntimeError(f'{line.split()[0]}: {result["error"]}')
        return result

    def command(self, text):
        return self.call('command ' + text)

    def magic(self):
        return self.call('arcana-status')

    def state(self):
        return self.wait(self.magic, lambda s: s['snapshot'] is not None)['snapshot']['state']

    def wait(self, sample, predicate, timeout=15):
        until = time.monotonic() + timeout
        while time.monotonic() < until:
            value = sample()
            if predicate(value):
                return value
            time.sleep(.035)
        (self.root / 'failed-state.json').write_text(json.dumps(value, ensure_ascii=False, indent=2), encoding='utf-8')
        raise AssertionError('Native state did not meet the expected condition')

    def select(self, spell):
        for _ in range(20):
            if self.state()['selected'] == spell:
                return
            self.call('key r tap')
            time.sleep(.075)
        raise AssertionError('Spell was not selectable: ' + spell)

    def shot(self, name):
        self.call('screenshot ' + str(self.root / (name + '.png')))

    def mobs(self):
        return [m for m in self.call('mobs 120')['near'] if m['kind'] == 'minecraft:iron_golem' and m['health'] > 0]

    def stop(self):
        if self.process.poll() is None:
            try:
                if self.stream:
                    self.call('key shift up')
                    self.call('key ctrl up')
                    self.call('quit')
                else:
                    self.process.terminate()
                self.process.wait(timeout=25)
            except (OSError, ValueError, RuntimeError, subprocess.TimeoutExpired):
                self.process.terminate()
                self.process.wait(timeout=10)
        if self.stream:
            self.stream.close()
        if self.connection:
            self.connection.close()
        self.log.close()
        if self.process.returncode:
            raise AssertionError('Native client exited with an error; see ' + str(self.root / 'session.log'))


def visible(s, spell):
    return any(v['spell_id'] == spell for v in s['spell_visuals']) or any(v['effect']['spell_id'] == spell for v in s['fire_fields'])


def summon(game, x=.5, y=75, z=4.5, physics=False):
    game.command(f'summon minecraft:iron_golem {x} {y} {z} {{NoAI:{0 if physics else 1}b,PersistenceRequired:1b}}')
    if physics:
        game.command('attribute @e[type=minecraft:iron_golem,limit=1,sort=nearest] minecraft:movement_speed base set 0')


def arena(game):
    for text in ['gamerule minecraft:send_command_feedback false', 'gamerule minecraft:spawn_mobs false',
                 'time set 1000', 'fill -20 74 -16 20 74 24 minecraft:stone',
                 'fill -20 75 -16 20 88 24 minecraft:air', 'tp @s 0.5 75 -6.5']:
        game.command(text)
    game.call('look 0 0')
    time.sleep(.3)


def validate(root, binary, spell):
    host = observer = None
    try:
        host = Game(root / 'host', binary)
        arena(host)
        port = host.call('lan open 0')['port']
        observer = Game(root / 'observer', binary, f'127.0.0.1:{port}')
        host.command('gamemode spectator ArcanaWitness')
        host.command('tp ArcanaWitness 16.5 82 -19.5')
        observer.wait(lambda: observer.call('status'), lambda s: abs(s['player']['position'][0] - 16.5) < .1)
        observer.call('look-at 0.5 78 -1.5')
        host.select(spell)
        equipped = host.state()['equipped']
        assert len(equipped) == 15, equipped
        if spell != 'fire_dash':
            host.call('key v tap')
            time.sleep(.085)
            host.call('key x tap')
            cancelled = host.wait(host.magic, lambda s: s['snapshot']['cast'] is None and s['snapshot']['state']['mana'] == 160 and s['snapshot']['state']['cooldowns'].get(spell, 0) > 0)
            assert cancelled['server_fire']['effects'] == 0
            host.call('arcana-demo')
            host.wait(host.magic, lambda s: s['snapshot']['state']['cooldowns'].get(spell, 0) == 0)
        if spell in ['flame_shield', 'phoenix_form', 'fire_avatar']:
            host.command('gamemode survival @s')
        if spell == 'fire_dash':
            summon(host, z=-1.5)
            host.command('fill -2 75 1 2 78 1 minecraft:stone')
        elif spell == 'fire_storm':
            summon(host, z=-3.5, physics=True)
        elif spell == 'fire_ring':
            summon(host, z=-3.5)
        elif spell not in ['flame_shield', 'meteor_rain', 'fire_avatar']:
            summon(host, z=8.5 if spell in ['spark', 'fireball'] else 4.5)
        if spell == 'fire_blast':
            host.command('setblock 3 75 4 minecraft:chest')
            host.command('setblock 4 75 4 minecraft:bedrock')
        if spell == 'inferno_beam':
            summon(host, z=10.5)
            host.command('fill -3 75 8 3 81 8 minecraft:stone')
        before = host.state()
        if spell == 'fire_dash':
            host.call('key ctrl down')
            host.wait(host.magic, lambda s: s['snapshot']['state']['cooldowns'].get(spell, 0) > 0)
            host.call('key ctrl up')
        else:
            host.call('key v tap')
        if spell in ['solar_spear', 'meteor_rain']:
            host.wait(host.magic, lambda s: s['snapshot']['cast'] is not None and s['snapshot']['cast']['phase'] == 'Preparation')
            fixed = host.call('status')['player']['position']
            host.call('key w down')
            host.call('key space down')
            time.sleep(.4)
            held = host.call('status')['player']['position']
            host.call('release')
            assert sum((a - b)**2 for a, b in zip(fixed, held)) < .001, (fixed, held)
        if spell == 'solar_spear':
            observed = observer.magic()
            host.wait(host.magic, lambda s: s['snapshot']['state']['xp'] > before['xp'])
        elif spell in ['flame_wall', 'flame_shield', 'fire_dash', 'fire_ring', 'fire_blast',
                       'inferno_beam', 'fire_storm', 'phoenix_form', 'meteor_rain', 'fire_avatar']:
            observed = observer.wait(observer.magic, lambda s: any(v['effect']['spell_id'] == spell and v['effect']['age'] >= (28 if spell == 'fire_ring' else 8) for v in s['fire_fields']))
            field = next(v['effect'] for v in observed['fire_fields'] if v['effect']['spell_id'] == spell)
            p = field['position']
            height = 5 if spell in ['fire_storm', 'flame_wall'] else 1.5
            observer.call(f'look-at {p[0]} {p[1] + height} {p[2]}')
        else:
            observed = observer.wait(observer.magic, lambda s: any(v['spell_id'] == spell and v['stage'] != 'Preparation' for v in s['spell_visuals']))
            observer.call('look-at 0.5 76.5 4.5')
        (root / 'released.json').write_text(json.dumps({'owner': host.magic(), 'observer': observed,
            'status': host.call('status'), 'mobs': host.mobs()}, ensure_ascii=False, indent=2), encoding='utf-8')
        observer.shot(spell + '-observer')
        host.shot(spell + '-owner')
        if spell == 'flame_shield':
            host.wait(host.magic, lambda s: s['snapshot']['fire']['shield'])
            host.command('damage @s 10 minecraft:generic')
            health = host.wait(lambda: host.call('status'), lambda s: s['player']['health'] < 20)
            assert abs(health['player']['health'] - 15.2) < .01, health['player']
            (root / 'shield-damage.json').write_text(json.dumps(health, indent=2))
        elif spell in ['phoenix_form', 'fire_avatar']:
            host.wait(host.magic, lambda s: s['flight']['may_fly'])
            if spell == 'phoenix_form':
                time.sleep(1)
                assert host.call('status')['player']['position'][1] > 75.1
                host.call('look 0 55')
                time.sleep(.2)
                host.call('mouse right click')
                host.wait(host.magic, lambda s: any(v['effect']['spell_id'] == spell and v['effect']['stage'] == 2 for v in s['fire_fields']), 12)
                observer.shot('phoenix-impact')
                host.wait(host.magic, lambda s: not s['flight']['may_fly'])
                host.wait(host.mobs, lambda m: any(t['health'] < 100 for t in m))
            else:
                host.call('key space down')
                time.sleep(.35)
                host.call('key space up')
                current = host.call('status')['player']['position']
                assert current[1] > 75.2, current
                host.call('look 0 0')
                summon(host, y=current[1], z=current[2] + 15)
                time.sleep(.15)
                mana = host.state()['mana']
                host.call('mouse left click')
                host.wait(host.magic, lambda s: any(b['kind'] == 'Empowered' for b in s['server_fire']['projectile_states']))
                shot = host.state()
                assert mana - 6.1 <= shot['mana'] < mana - 3, (mana, shot)
                host.call('mouse left click')
                assert host.state()['mana'] >= shot['mana'] - .01
                host.wait(host.mobs, lambda m: any(t['health'] <= 80 for t in m))
        elif spell == 'meteor_rain':
            meteor = host.wait(host.magic, lambda s: any(b['kind'] == 'Meteor' for b in s['server_fire']['projectile_states']))
            b = next(b for b in meteor['server_fire']['projectile_states'] if b['kind'] == 'Meteor')
            summon(host, x=b['position'][0], y=b['position'][1] - 3, z=b['position'][2])
            host.wait(host.magic, lambda s: s['snapshot']['state']['xp'] > before['xp'], 8)
            host.wait(host.magic, lambda s: s['owned_fire_cells'] > 0, 8)
            observer.call(f"look-at {b['position'][0]} {b['position'][1] - 3} {b['position'][2]}")
            observer.shot('meteor-impact')
        elif spell != 'flame_shield':
            host.wait(host.magic, lambda s: s['snapshot']['state']['xp'] > before['xp'], 15)
        if spell == 'inferno_beam':
            host.call('key x tap')
            host.wait(host.magic, lambda s: not any(v['effect']['spell_id'] == spell for v in s['fire_fields']))
            far = [m for m in host.mobs() if m['pos'][2] > 9]
            assert far and all(m['health'] == 100 for m in far), far
        after = host.magic()
        mobs = host.mobs()
        if spell != 'flame_shield':
            assert mobs and any(m['health'] < 100 for m in mobs), mobs
        if spell == 'flame_wall':
            assert observed['owned_fire_cells'] == 25, observed
        if spell == 'fire_dash':
            position = host.call('status')['player']['position']
            assert -1.5 < position[2] < .71, position
            assert after['snapshot']['state']['mana'] == 160
        assert observer.state()['school'] is None and observer.state()['xp'] == 0
        if spell == 'fire_blast':
            for block, expected in [('3 75 4', 'minecraft:chest'), ('4 75 4', 'minecraft:bedrock')]:
                value = host.call('block ' + block)
                assert value['block'] == expected, value
                (root / ('protected-' + block.replace(' ', '-') + '.json')).write_text(json.dumps(value, indent=2))
        host.command('kill @s')
        host.wait(host.magic, lambda s: s['server_fire']['effects'] == 0 and s['server_projectiles'] == 0)
        if spell in ['fire_avatar', 'phoenix_form']:
            host.call('respawn')
            host.wait(host.magic, lambda s: s['snapshot'] is not None and not s['snapshot']['fire']['flight'] and s['flight'] is not None and not s['flight']['may_fly'])
        report = {'spell': spell, 'quality': os.environ.get('ARCANA_QUALITY', 'high'),
                  'reduced_effects': bool(os.environ.get('ARCANA_REDUCED_EFFECTS')),
                  'before': before, 'after': after, 'mobs': mobs,
                  'observer': observed, 'result': 'passed'}
        (root / 'report.json').write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding='utf-8')
        print(json.dumps({'spell': spell, 'result': 'passed', 'run': str(root)}), flush=True)
        return report
    finally:
        for game in [observer, host]:
            if game:
                game.stop()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--spell', choices=SPELLS, action='append')
    args = parser.parse_args()
    root = FORK / '.local/fire-school-validation' / uuid.uuid4().hex
    binary = root / 'bin' / ('horizon-client.exe' if os.name == 'nt' else 'horizon-client')
    binary.parent.mkdir(parents=True)
    shutil.copy2(FORK / 'target/fast' / binary.name, binary)
    selected = args.spell or SPELLS
    report = {'run': str(root), 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'cases': []}
    print(json.dumps({'run': str(root), 'cases': len(selected)}), flush=True)
    for spell in selected:
        case = root / spell
        case.mkdir()
        report['cases'].append(validate(case, binary, spell))
        (root / 'report.json').write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding='utf-8')
    print(json.dumps({'run': str(root), 'result': 'passed', 'count': len(report['cases'])}), flush=True)


if __name__ == '__main__':
    main()
