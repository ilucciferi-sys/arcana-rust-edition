#!/usr/bin/env python3
"""Native Arcana poses: two isolated clients, actual model bones and screenshots."""
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


def validate(root, binary):
    host = observer = None
    evidence = {}
    def save():
        (root / 'report.json').write_text(json.dumps(evidence, ensure_ascii=False, indent=2), encoding='utf-8')
    try:
        host = school.Game(root / 'host', binary)
        school.arena(host)
        port = host.call('lan open 0 commands')['port']
        observer = school.Game(root / 'observer', binary, f'127.0.0.1:{port}')
        host.command('gamemode spectator ArcanaWitness')
        host.command('tp ArcanaWitness 3.5 75 -2.5')
        observer.wait(lambda: observer.call('status'), lambda s: abs(s['player']['position'][0]-3.5)<.1)
        observer.call('look-at 0.5 76 -6.5')
        catalog = {s['id']: s for s in json.loads((school.FORK/'arcana/assets/data/spells.json').read_text(encoding='utf-8'))}
        for spell in ['fireball', 'flame_wall', 'inferno_beam', 'meteor_rain', 'solar_spear', 'phoenix_form']:
            host.call('arcana-demo')
            host.select(spell)
            host.call('look 0 0')
            time.sleep(.25)
            host.call('key v tap')
            age = catalog[spell]['castTime'] * .52
            own = host.wait(lambda: pose(host,spell,True), lambda p: p is not None and p['age']>=age)
            seen = observer.wait(lambda: pose(observer,spell,False), lambda p: p is not None and p['age']>=age)
            for p in [own,seen]:
                assert all(math.isfinite(v) for values in p['bones'].values() for v in values)
                assert max(abs(p['bones'][arm][3]) for arm in ['right_arm','left_arm']) > .2, p
                assert abs(p['bends']['right_arm']) > .05, p
            host.shot(spell+'-arms')
            observer.shot(spell+'-body')
            evidence[spell] = {'owner':own,'witness':seen}
            if spell == 'inferno_beam':
                channel = host.wait(host.magic, lambda m: m['snapshot']['cast'] is not None and m['snapshot']['cast']['phase']=='Channeling')
                a = observer.wait(lambda: pose(observer,spell,False), lambda p: p is not None and p['age']>catalog[spell]['castTime']+3)
                time.sleep(.4)
                b = pose(observer,spell,False)
                assert b and abs(a['bones']['right_arm'][3]-b['bones']['right_arm'][3])<.03, (a,b)
                evidence['channel_hold'] = [a,b]
                host.shot('inferno-beam-channel-arms')
                observer.shot('inferno-beam-channel-body')
                host.call('options-set mainHand left')
                mirrored = host.wait(lambda: pose(host,spell,True), lambda p: p is not None and abs(p['bones']['left_arm'][3]-a['bones']['right_arm'][3])<.05)
                evidence['left_hand'] = mirrored
                host.shot('inferno-beam-left-hand')
                host.call('options-set mainHand right')
                observer.stop()
                observer = None
                observer = school.Game(root/'late-observer',binary,f'127.0.0.1:{port}')
                late = observer.wait(lambda: pose(observer,spell,False),lambda p: p is not None and p['phase']=='Channeling')
                assert late['age']>catalog[spell]['castTime']+10,late
                evidence['late_join_during_channel'] = late
                host.wait(host.magic,lambda m: m['snapshot']['cast'] is None,20)
                host.wait(lambda: host.magic()['cast_poses'],lambda p: not p)
            else:
                host.call('key x tap')
                host.wait(host.magic,lambda m: m['snapshot']['cast'] is None)
                host.wait(lambda: host.magic()['cast_poses'],lambda p: not p)
                observer.wait(lambda: observer.magic()['cast_poses'],lambda p: not p)
                assert host.state()['mana']==160
            save()
        host.call('arcana-demo')
        host.select('fire_avatar')
        host.command('tp @s 0.5 75 -6.5')
        time.sleep(.2)
        start = host.call('status')['player']['position']
        host.call('key v tap')
        host.call('key w down')
        time.sleep(.45)
        walking = pose(host,'fire_avatar',True)
        end = host.call('status')['player']['position']
        assert walking and end[2]-start[2]>.2,(walking,start,end)
        assert max(abs(walking['bones'][leg][3]) for leg in ['right_leg','left_leg'])>.05,walking
        host.call('release')
        host.call('key x tap')
        host.wait(host.magic,lambda m: m['snapshot']['cast'] is None)
        evidence['mobile_cast_with_locomotion'] = {'pose':walking,'start':start,'end':end}
        # Death must stop a remote body pose and not leak it through respawn.
        host.call('arcana-demo')
        host.select('solar_spear')
        host.call('key v tap')
        observer.wait(lambda: pose(observer,'solar_spear',False),lambda p: p is not None and p['age']>5)
        host.command('kill @s')
        observer.wait(lambda: observer.magic()['cast_poses'],lambda p: not p)
        time.sleep(.3)
        host.call('respawn')
        host.wait(host.magic,lambda m: m['snapshot'] is not None and m['snapshot']['cast'] is None)
        assert not host.magic()['cast_poses']
        evidence['death_respawn_cleanup'] = True
        evidence['result'] = 'passed'
        save()
    except BaseException:
        evidence['result']='failed'
        save()
        raise
    finally:
        for game in [observer,host]:
            if game: game.stop()


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--binary',type=Path,default=school.FORK/'target/fast/horizon-client.exe')
    args=parser.parse_args()
    root=school.FORK/'.local/cast-animation-validation'/uuid.uuid4().hex
    root.mkdir(parents=True)
    binary=root/args.binary.name
    shutil.copy2(args.binary,binary)
    (root/'binary.sha256').write_text(hashlib.sha256(binary.read_bytes()).hexdigest(),encoding='ascii')
    print(root,flush=True)
    validate(root,binary)
    print('PASS: original casts, first-person arms, LAN body poses, channel, cancellation, death/respawn',flush=True)


if __name__=='__main__': main()
