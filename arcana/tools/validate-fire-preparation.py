#!/usr/bin/env python3
"""Native Fire charge shapes, textured seals, cancellation and awakening."""
from pathlib import Path
import argparse
import hashlib
import importlib.util
import json
import math
import os
import shutil
import time
import uuid

spec=importlib.util.spec_from_file_location('fire_school_validation',Path(__file__).with_name('validate-fire-school.py'))
school=importlib.util.module_from_spec(spec)
spec.loader.exec_module(school)
SPELLS=[s for s in school.SPELLS if s!='fire_dash']


def charge(game,spell,own):
    return next((p for p in game.magic()['preparations'] if p['spell']==spell and p['own']==own),None)


def validate(root,binary,spells,skip_ritual):
    host=observer=None
    report={'result':'running','binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),
            'quality':os.environ['ARCANA_QUALITY'],'reduced':bool(os.environ.get('ARCANA_REDUCED_EFFECTS')),'spells':{}}
    def save():
        (root/'report.json').write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf-8')
    try:
        host=school.Game(root/'host',binary)
        school.arena(host)
        port=host.call('lan open 0 commands')['port']
        observer=school.Game(root/'observer',binary,f'127.0.0.1:{port}')
        host.command('gamemode spectator ArcanaWitness')
        host.command('tp ArcanaWitness 3.5 76 -3.5')
        observer.wait(lambda:observer.call('status'),lambda s:abs(s['player']['position'][0]-3.5)<.1)
        observer.call('look-at 0.5 76 -6.5')
        catalog={s['id']:s for s in json.loads((school.FORK/'arcana/assets/data/spells.json').read_text(encoding='utf-8'))}
        families=set()
        for spell in spells:
            host.call('arcana-demo')
            host.select(spell)
            host.call('look 0 0')
            time.sleep(.1)
            host.call('key v tap')
            minimum=.35 if catalog[spell]['castTime']<15 else .58
            own=host.wait(lambda:charge(host,spell,True),lambda p:p is not None and p['progress']>=minimum)
            seen=observer.wait(lambda:charge(observer,spell,False),lambda p:p is not None and p['progress']>=minimum)
            for p in [own,seen]:
                assert p['marks']>0 and p['marks']<=640 and p['opacity']>0,p
                assert all(math.isfinite(v) for v in p['palm']+p['feet']),p
                assert p['textures_ready'],p
            assert own['first_person'] and not seen['first_person'],(own,seen)
            assert own['family']==seen['family'],(own,seen)
            families.add(own['family'])
            if catalog[spell]['castTime']>=15:
                host.shot(spell+'-charge-first-person')
                observer.shot(spell+'-charge-witness')
            host.call('key x tap')
            host.wait(host.magic,lambda m:m['snapshot']['cast'] is None)
            host.wait(lambda:host.magic()['preparations'],lambda p:not p)
            observer.wait(lambda:observer.magic()['preparations'],lambda p:not p)
            report['spells'][spell]={'owner':own,'witness':seen}
            save()
        assert len(families)==len(spells),(families,spells)

        # Rejoin with the already-running client, keeping preparation under its
        # original 70-tick limit instead of extending gameplay for the fixture.
        observer.call('disconnect')
        host.call('arcana-demo')
        host.select('solar_spear')
        host.call('key v tap')
        observer.call(f'connect 127.0.0.1:{port} ArcanaWitness')
        late=observer.wait(lambda:charge(observer,'solar_spear',False),lambda p:p is not None and p['progress']>.1,10)
        assembled=observer.wait(lambda:charge(observer,'solar_spear',False),lambda p:p is not None and p['progress']>.6 and p['planes']>0)
        report['late_join']={'early':late,'assembled':assembled}
        host.call('key x tap')
        host.wait(host.magic,lambda m:m['snapshot']['cast'] is None)
        host.wait(lambda:host.magic()['preparations'],lambda p:not p)
        observer.wait(lambda:observer.magic()['preparations'],lambda p:not p)
        assert host.state()['mana']==160
        report['cancel_cleanup']=True

        host.call('arcana-demo')
        host.call('key v tap')
        observer.wait(lambda:charge(observer,'solar_spear',False),lambda p:p is not None)
        host.command('kill @s')
        observer.wait(lambda:observer.magic()['preparations'],lambda p:not p)
        host.call('respawn')
        host.wait(host.magic,lambda m:m['snapshot'] is not None and m['snapshot']['cast'] is None)
        assert not host.magic()['preparations']
        report['death_respawn_cleanup']=True

        if not skip_ritual:
            for command in ['fill -6 74 -6 6 74 6 minecraft:stone','fill -6 75 -6 6 85 6 minecraft:air',
                            'time set 1000','setblock 0 74 0 minecraft:obsidian']:
                host.command(command)
            for distance,block in [(1,'obsidian'),(2,'magma_block')]:
                for x,z in [(distance,0),(-distance,0),(0,distance),(0,-distance)]:
                    host.command(f'setblock {x} 74 {z} minecraft:{block}')
            for x in [-2,2]:
                for z in [-2,2]:
                    host.command(f'setblock {x} 74 {z} minecraft:obsidian')
                    host.command(f'setblock {x} 75 {z} minecraft:campfire[lit=true]')
            host.command('setblock 3 74 0 minecraft:lava[level=0]')
            host.command('gamemode creative ArcanaWitness')
            host.command('tp ArcanaWitness 0.5 75 0.5')
            host.command('tp @s 4.5 77 -4.5')
            host.call('look-at 0.5 75.5 0.5')
            observer.call('look 0 65')
            observer.call('key shift down')
            active=observer.wait(observer.magic,lambda m:m['snapshot'] is not None and m['snapshot']['ritual'] is not None)
            observer.call('key shift up')
            host.wait(host.magic,lambda m:m['ritual_visuals']>0)
            host.shot('fire-awakening-sigil')
            observer.shot('fire-awakening-first-person')
            host.command('tp ArcanaWitness 8.5 75 0.5')
            observer.wait(observer.magic,lambda m:m['snapshot']['ritual'] is None)
            time.sleep(.3)
            host.shot('fire-awakening-cancelled')
            host.wait(host.magic,lambda m:m['ritual_visuals']==0)
            assert observer.state()['school'] is None
            report['ritual_cancelled']=active['snapshot']['ritual']
            host.command('tp ArcanaWitness 0.5 75 0.5')
            observer.call('key shift down')
            observer.wait(observer.magic,lambda m:m['snapshot']['ritual'] is not None)
            observer.call('key shift up')
            observer.wait(observer.magic,lambda m:m['snapshot']['ritual'] is not None and m['snapshot']['ritual']['progress']>65)
            host.shot('fire-awakening-built-sigil')
            observer.wait(observer.magic,lambda m:m['snapshot']['state']['school']=='fire',20)
            host.wait(host.magic,lambda m:m['ritual_visuals']==0)
            assert observer.state()['xp']==0
            report['ritual_completed']=True
        report['result']='passed'
        save()
    except BaseException:
        report['result']='failed'
        save()
        raise
    finally:
        for game in [observer,host]:
            if game:game.stop()


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--quality',choices=['performance','high','cinematic'],default='high')
    parser.add_argument('--reduced',action='store_true')
    parser.add_argument('--spell',choices=SPELLS,action='append')
    parser.add_argument('--skip-ritual',action='store_true')
    args=parser.parse_args()
    os.environ['ARCANA_QUALITY']=args.quality
    if args.reduced:os.environ['ARCANA_REDUCED_EFFECTS']='1'
    else:os.environ.pop('ARCANA_REDUCED_EFFECTS',None)
    root=school.FORK/'.local/fire-preparation-validation'/uuid.uuid4().hex
    root.mkdir(parents=True)
    binary=root/'horizon-client.exe'
    shutil.copy2(school.FORK/'target/fast/horizon-client.exe',binary)
    print(root,flush=True)
    validate(root,binary,args.spell or SPELLS,args.skip_ritual)
    print('PASS: distinct Fire preparation, posed palms, seals, late join and cleanup',flush=True)


if __name__=='__main__':main()
