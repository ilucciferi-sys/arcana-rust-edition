#!/usr/bin/env python3
"""Fire body materials and flight rigs in two isolated, silent native clients."""
from pathlib import Path
import argparse
import hashlib
import importlib.util
import json
import math
import shutil
import time
import uuid

spec=importlib.util.spec_from_file_location('fire_school_validation',Path(__file__).with_name('validate-fire-school.py'))
school=importlib.util.module_from_spec(spec)
spec.loader.exec_module(school)


def body(game,kind,own):
    return next((f for f in game.magic()['body_forms'] if f['kind']==kind and f['own']==own),None)


def ready(form):
    return form is not None and form['material_ready'] and form['opacity']>.8


def validate(root,binary,quick=False):
    host=observer=None
    report={'result':'running','quick':quick,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest()}
    def save():
        (root/'report.json').write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf-8')
    try:
        host=school.Game(root/'host',binary)
        school.arena(host)
        host.command('gamemode survival @s')
        for slot,item in [('head','diamond_helmet'),('chest','diamond_chestplate'),('legs','diamond_leggings'),('feet','diamond_boots')]:
            host.command(f'item replace entity @s armor.{slot} with minecraft:{item}')
        port=host.call('lan open 0 commands')['port']
        observer=school.Game(root/'observer',binary,f'127.0.0.1:{port}')
        host.command('gamemode spectator ArcanaWitness')
        host.command('tp ArcanaWitness 4.5 77 -12.5')
        observer.wait(lambda:observer.call('status'),lambda s:abs(s['player']['position'][0]-4.5)<.1)
        observer.call('look-at 0.5 76 -6.5')
        observer.shot('normal-armored-body')
        host.select('fire_avatar')
        host.call('key v tap')
        own=host.wait(lambda:body(host,'Avatar',True),ready)
        seen=observer.wait(lambda:body(observer,'Avatar',False),ready)
        assert len(observer.magic()['body_forms'])==1
        host.wait(host.magic,lambda m:m['snapshot']['cast'] is None)
        host.wait(host.magic,lambda m:not m['cast_poses'])
        # Original MagicStatuses.fly lifts the caster off the ground on release.
        host.wait(lambda:body(host,'Avatar',True),lambda f:ready(f) and f['flying'])
        observer.wait(lambda:body(observer,'Avatar',False),lambda f:ready(f) and f['flying'])
        hover=host.wait(lambda:body(host,'Avatar',True),lambda f:ready(f) and abs(math.degrees(f['bones']['body'][3])-8)<.1)
        assert abs(math.degrees(hover['bones']['head'][3]))<10,hover
        observer.shot('avatar-hover-witness')
        host.shot('avatar-first-person')
        host.call('key f5 tap')
        host.call('key f5 tap')
        host.shot('avatar-third-person')
        host.call('key f5 tap')
        start=host.call('status')['player']['position']
        host.call('key w down')
        host.call('key space down')
        fast=host.wait(lambda:body(host,'Avatar',True),lambda f:ready(f) and math.degrees(f['bones']['body'][3])>12)
        host.call('release')
        end=host.call('status')['player']['position']
        assert end[2]>start[2]+.1 and end[1]>start[1],(start,end)
        host.command('tp ArcanaWitness 4.5 78 -12.5')
        observer.call(f'look-at {end[0]} {end[1]+1} {end[2]}')
        observer.shot('avatar-powered-flight')
        report['avatar']={'owner':own,'witness':seen,'hover':hover,'powered':fast,'start':start,'end':end}
        # Landing disables vanilla flight, but keeps the elemental material.
        host.call('key shift down')
        grounded=host.wait(lambda:body(host,'Avatar',True),lambda f:ready(f) and not f['flying'])
        host.call('release')
        report['grounded_avatar']=grounded
        for _ in range(2):
            host.call('key space down')
            time.sleep(.10)
            host.call('key space up')
            time.sleep(.07)
        host.wait(lambda:body(host,'Avatar',True),lambda f:ready(f) and f['flying'])
        print('PASS: fire material, shared hover, powered flight, first/third person',flush=True)
        save()
        # Casting replaces the flight pose while the form's material remains.
        host.select('fireball')
        host.call('key v tap')
        cast=host.wait(host.magic,lambda m:any(p['spell']=='fireball' and p['age']>4 for p in m['cast_poses']))
        assert body(host,'Avatar',True)['material_ready']
        host.call('key x tap')
        host.wait(host.magic,lambda m:m['snapshot']['cast'] is None)
        host.wait(lambda:body(host,'Avatar',True),lambda f:ready(f) and abs(math.degrees(f['bones']['body'][3])-8)<.1)
        report['cast_over_flight']=True
        # A new connection receives the active form without its release packet.
        observer.stop()
        observer=None
        observer=school.Game(root/'late-observer',binary,f'127.0.0.1:{port}')
        late=observer.wait(lambda:body(observer,'Avatar',False),ready)
        report['late_join']=late
        assert late['age']>40
        if quick:
            host.command('kill @s')
            host.wait(host.magic,lambda m:m['snapshot'] is not None and not m['snapshot']['fire']['flight'])
            host.call('respawn')
            host.wait(host.magic,lambda m:m['snapshot'] is not None and not m['snapshot']['fire']['flight'])
        else:
            print('Waiting for natural Avatar expiry (120 seconds from release)',flush=True)
            host.wait(host.magic,lambda m:not m['body_forms'] and not m['snapshot']['fire']['flight'],125)
            report['natural_avatar_expiry']=True
        observer.wait(observer.magic,lambda m:not m['body_forms'])
        host.command('tp @s 0.5 75 -6.5')
        observer.call('look-at 0.5 76 -6.5')
        observer.shot('avatar-normal-body-after')
        report['avatar_cleanup']=True
        save()
        # Overlapping effects: the newer Phoenix owns the pose, and an older
        # still-active Avatar must keep its material after Phoenix impact.
        host.call('arcana-demo')
        host.command('tp @s 0.5 75 -6.5')
        host.call('look 0 0')
        host.select('fire_avatar')
        host.call('key v tap')
        host.wait(lambda:body(host,'Avatar',True),ready)
        host.wait(host.magic,lambda m:m['snapshot']['cast'] is None)
        host.call('arcana-demo')
        host.select('phoenix_form')
        host.call('key v tap')
        host.wait(lambda:body(host,'Phoenix',True),ready)
        observer.wait(lambda:body(observer,'Phoenix',False),ready)
        host.command('tp @s 0.5 120 -6.5')
        host.wait(lambda:host.call('status'),lambda s:s['player']['position'][1]>119)
        host.command('tp ArcanaWitness 7.5 117 -14.5')
        observer.call('look-at 0.5 121 -6.5')
        observer.shot('phoenix-rising')
        host.call('look 25 55')
        time.sleep(.22)
        host.call('mouse right click')
        dive=host.wait(lambda:body(host,'Phoenix',True),lambda f:ready(f) and f['stage']==1)
        assert abs(dive['bones']['body'][3]-hover['bones']['body'][3])>.5,dive
        position=host.call('status')['player']['position']
        observer.call(f'look-at {position[0]} {position[1]+.7} {position[2]}')
        observer.shot('phoenix-dive')
        host.wait(host.magic,lambda m:body(host,'Avatar',True) is not None and not m['snapshot']['fire']['flight'],12)
        remaining=observer.wait(lambda:body(observer,'Avatar',False),lambda f:ready(f) and not f['flying'])
        position=host.call('status')['player']['position']
        host.command(f'tp ArcanaWitness {position[0]+8} {position[1]+4} {position[2]-8}')
        observer.wait(lambda:observer.call('status'),lambda s:abs(s['player']['position'][0]-position[0]-8)<.1)
        observer.call(f'look-at {position[0]} {position[1]+1} {position[2]}')
        observer.shot('phoenix-impact-remaining-avatar')
        report['phoenix']={'dive':dive,'older_avatar_retained':remaining}
        assert observer.state()['school'] is None
        # Death/respawn clears the remaining effect, independently of its lease.
        host.command('kill @s')
        observer.wait(observer.magic,lambda m:not m['body_forms'])
        host.call('respawn')
        host.wait(host.magic,lambda m:m['snapshot'] is not None and not m['snapshot']['fire']['flight'] and not m['body_forms'])
        report['death_respawn_cleanup']=True
        report['result']='passed'
        save()
        print('PASS: late join, form cleanup, Phoenix dive, death/respawn',flush=True)
    except BaseException as error:
        report['result']='failed'
        report['failure']=str(error)
        for name,game in [('host',host),('observer',observer)]:
            if game:
                try:
                    (root/f'{name}-failed-state.json').write_text(json.dumps({'magic':game.magic(),'status':game.call('status')},ensure_ascii=False,indent=2),encoding='utf-8')
                except Exception:
                    pass
        save()
        raise
    finally:
        for game in [observer,host]:
            if game:game.stop()


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--quick',action='store_true',help='Use death instead of waiting for natural Avatar expiry')
    args=parser.parse_args()
    root=school.FORK/'.local/fire-forms-validation'/uuid.uuid4().hex
    root.mkdir(parents=True)
    binary=root/'horizon-client.exe'
    shutil.copy2(school.FORK/'target/fast/horizon-client.exe',binary)
    print(root,flush=True)
    validate(root,binary,args.quick)


if __name__=='__main__':main()
