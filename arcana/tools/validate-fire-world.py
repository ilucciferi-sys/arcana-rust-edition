#!/usr/bin/env python3
"""Native Fire regression: PvP, protection, late joins and owned-fire persistence."""
import importlib.util
import json
from pathlib import Path
import shutil
import time
import uuid

spec = importlib.util.spec_from_file_location('fire_school', Path(__file__).with_name('validate-fire-school.py'))
school = importlib.util.module_from_spec(spec)
spec.loader.exec_module(school)
Game, arena, summon = school.Game, school.arena, school.summon


class Evidence(dict):
    def __init__(self, path):
        super().__init__(result='running')
        self.path = path

    def __setitem__(self, key, value):
        super().__setitem__(key, value)
        self.path.write_text(json.dumps(self, ensure_ascii=False, indent=2), encoding='utf-8')


def healthy(host, game):
    host.command('tp ArcanaWitness 0.5 75 4.5')
    game.wait(lambda: game.call('status'), lambda s: abs(s['player']['position'][2] - 4.5) < .1)
    host.command('setblock 0 75 4 minecraft:water')
    host.command('effect clear ArcanaWitness')
    time.sleep(.3)
    host.command('effect give ArcanaWitness minecraft:instant_health 1 8 true')
    game.wait(lambda: game.call('status'), lambda s: s['player']['health'] == 20)
    host.command('effect clear ArcanaWitness')
    host.command('setblock 0 75 4 minecraft:air')
    time.sleep(.3)


def cast(game, spell):
    game.call('release')
    game.call('look 0 0')
    game.call('arcana-demo')
    game.select(spell)
    game.wait(game.magic, lambda s: s['snapshot']['state']['cooldowns'].get(spell, 0) == 0)
    before = game.state()
    game.call('key v tap')
    game.wait(game.magic, lambda s: s['snapshot']['cast'] is None and s['snapshot']['state']['cooldowns'].get(spell, 0) > 0)
    return before


def run(root, binary):
    host = witness = None
    evidence = Evidence(root / 'report.json')
    try:
        host = Game(root / 'host', binary)
        arena(host)
        port = host.call('lan open 0 commands')['port']
        witness = Game(root / 'witness', binary, f'127.0.0.1:{port}')
        host.command('gamemode survival ArcanaWitness')
        host.command('tp ArcanaWitness 0.5 75 4.5')
        host.call('look 0 0')
        time.sleep(.2)
        witness.command('attribute @s minecraft:knockback_resistance base set 1')
        host.command('gamerule minecraft:pvp false')
        before = cast(host, 'ignition')
        time.sleep(.5)
        assert witness.call('status')['player']['health'] == 20
        assert host.state()['xp'] == before['xp']
        evidence['pvp_disabled'] = True
        host.command('gamerule minecraft:pvp true')
        host.command('team add fire_allies')
        host.command('team join fire_allies @a')
        before = cast(host, 'ignition')
        time.sleep(.4)
        assert witness.call('status')['player']['health'] == 20
        assert host.state()['xp'] == before['xp']
        evidence['allied_players'] = True
        host.command('team empty fire_allies')
        before = cast(host, 'ignition')
        hit = witness.wait(lambda: witness.call('status'), lambda s: s['player']['health'] < 20)
        host.wait(host.magic, lambda s: s['snapshot']['state']['xp'] == before['xp'] + 4)
        evidence['confirmed_player_hit'] = hit['player']
        healthy(host, witness)
        host.command('effect give ArcanaWitness minecraft:resistance 30 255 true')
        before = cast(host, 'spark')
        host.wait(host.magic, lambda s: s['server_projectiles'] == 0)
        time.sleep(.4)
        assert witness.call('status')['player']['health'] == 20
        assert host.state()['xp'] == before['xp'] + 4
        evidence['resistance_preserves_accepted_hit_reward'] = True
        host.command('effect clear ArcanaWitness')
        healthy(host, witness)
        host.call('arcana-demo')
        host.select('spark')
        before = host.state()
        host.call('key v tap')
        host.wait(host.magic, lambda s: any(b['kind'] == 'Spark' and b['position'][2] > -3 for b in s['server_fire']['projectile_states']))
        witness.command('damage @s 6 minecraft:generic')
        witness.wait(lambda: witness.call('status'), lambda s: s['player']['health'] < 15)
        host.wait(host.magic, lambda s: s['server_projectiles'] == 0 and s['snapshot']['cast'] is None)
        time.sleep(.3)
        assert host.state()['xp'] == before['xp']
        assert witness.call('status')['player']['health'] < 20
        evidence['rejected_damage_no_xp'] = True
        healthy(host, witness)
        host.command('gamemode survival @s')
        witness.call('arcana-demo')
        witness.wait(witness.magic, lambda s: s['snapshot']['state']['school'] == 'fire')
        witness.select('flame_shield')
        witness.call('key v tap')
        witness.wait(witness.magic, lambda s: s['snapshot']['fire']['shield'])
        host.command('effect clear ArcanaWitness')
        # Keep the spell's 40% mitigation, remove potion resistance so the
        # incoming damage and retaliation can be checked separately.
        before = cast(host, 'ignition')
        witness.wait(lambda: witness.call('status'), lambda s: s['player']['health'] < 20)
        host.wait(lambda: host.call('status'), lambda s: s['player']['health'] < 20)
        evidence['shield_player_retaliation'] = True
        host.command('gamemode creative @s')
        host.command('kill ArcanaWitness')
        witness.wait(lambda: witness.call('state'), lambda s: s['screen'] == 'dead')
        witness.call('respawn')
        witness.wait(witness.magic, lambda s: s['snapshot'] is not None and not s['snapshot']['fire']['shield'])
        # The observer now has its own deliberately granted school; subsequent
        # checks must preserve that identity rather than force it unawakened.
        witness_progress = witness.state()
        host.command('effect clear ArcanaWitness')
        host.command('gamemode creative ArcanaWitness')
        before = cast(host, 'ignition')
        assert host.state()['xp'] == before['xp']
        assert witness.call('status')['player']['health'] == 20
        evidence['creative_player'] = True
        # A long-lived form must appear for a client who connects after release.
        witness.stop()
        witness = None
        cast(host, 'fire_avatar')
        host.wait(host.magic, lambda s: s['snapshot']['fire']['avatar'])
        witness = Game(root / 'witness', binary, f'127.0.0.1:{port}', resume=True)
        late = witness.wait(witness.magic, lambda s: any(v['effect']['spell_id'] == 'fire_avatar' for v in s['fire_fields']))
        assert witness.state()['school'] == witness_progress['school']
        evidence['late_join_avatar'] = late['fire_fields']
        witness.stop()
        witness = None
        host.command('gamemode survival @s')
        host.command('kill @s')
        host.wait(host.magic, lambda s: s['server_fire']['effects'] == 0)
        host.call('respawn')
        host.wait(host.magic, lambda s: not s['flight']['may_fly'])
        host.command('gamemode creative @s')
        host.command('tp @s 0.5 75 -6.5')
        host.call('look 0 0')
        summon(host)
        cast(host, 'flame_wall')
        host.wait(host.magic, lambda s: s['owned_fire_cells'] == 25)
        progress = host.state()
        host.stop()
        host = None
        journal = root / 'host/saves/ArcanaFireSchool/arcana/owned-fire-minecraft_overworld.json'
        saved = json.loads(journal.read_text(encoding='utf-8'))
        assert len(saved['cells']) == 25 and len({c['owner'] for c in saved['cells']}) == 1
        host = Game(root / 'host', binary, resume=True)
        loaded = host.wait(host.magic, lambda s: s['owned_fire_cells'] == 25)
        assert host.state()['xp'] == progress['xp']
        assert loaded['server_fire']['effects'] == 0
        evidence['owned_fire_saved_and_reloaded'] = saved
        host.command('tp @s 0.5 75 1.5')
        time.sleep(.2)
        host.call('look 0 20.5')
        host.call('mouse left click')
        host.wait(host.magic, lambda s: s['owned_fire_cells'] == 24)
        evidence['manual_extinguish'] = True
        host.command('setblock 2 75 4 minecraft:stone')
        host.wait(host.magic, lambda s: s['owned_fire_cells'] == 23)
        # Contain the water so its normal fluid ticks cannot extinguish
        # unrelated test cells while the support check is being measured.
        for pos in ['0 75 4', '1 75 3', '1 75 5']:
            host.command('setblock ' + pos + ' minecraft:stone')
        host.command('setblock 1 75 4 minecraft:water')
        host.wait(host.magic, lambda s: s['owned_fire_cells'] == 22)
        host.command('setblock 5 74 4 minecraft:air')
        host.wait(host.magic, lambda s: s['owned_fire_cells'] == 21)
        evidence['water_replacement_support'] = True
        host.command('weather rain')
        host.wait(host.magic, lambda s: s['owned_fire_cells'] == 0)
        evidence['rain_extinguishes'] = True
        host.stop()
        host = None
        # Fresh world's policy is installed before any spell can modify it.
        host = Game(root / 'protected', binary, fire_config={'fire_ignites': False, 'spell_griefing': False})
        arena(host)
        summon(host)
        cast(host, 'fire_blast')
        host.wait(host.magic, lambda s: any(v['effect']['spell_id'] == 'fire_blast' and v['effect']['age'] > 0 for v in s['fire_fields']))
        assert host.magic()['owned_fire_cells'] == 0
        assert host.call('block 0 74 4')['block'] == 'minecraft:stone'
        evidence['griefing_and_fire_disabled'] = True
        host.stop()
        host = None
        evidence['result'] = 'passed'
        print(json.dumps({'result': 'passed', 'run': str(root), 'checks': len(evidence) - 1}), flush=True)
    finally:
        for game in [witness, host]:
            if game:
                game.stop()


def main():
    root = school.FORK / '.local/fire-world-validation' / uuid.uuid4().hex
    binary = root / 'bin/horizon-client.exe'
    binary.parent.mkdir(parents=True)
    shutil.copy2(school.FORK / 'target/fast/horizon-client.exe', binary)
    print(json.dumps({'run': str(root)}), flush=True)
    run(root, binary)


if __name__ == '__main__':
    main()
