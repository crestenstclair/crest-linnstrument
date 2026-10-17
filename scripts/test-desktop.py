#!/usr/bin/env python3
"""Linux Tauri acceptance through native WebDriver input, real worker and sensors.

Requires WebKitWebDriver, xdotool, a display (Xvfb is sufficient),
the desktop executable and scripts/build-native.sh. No MIDI or audio hardware.
JavaScript only observes the rendered application's snapshots/geometry; it never
dispatches synthetic DOM gestures, invokes bridge commands or generates MIDI.
"""
import base64
import json
import os
import pathlib
import subprocess
import time
import urllib.error
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[1]
ARTIFACTS = pathlib.Path(os.environ.get('LINNSTRUMENT_DESKTOP_ARTIFACTS', ROOT / 'target/desktop-acceptance')).resolve()
BINARY = pathlib.Path(os.environ.get('LINNSTRUMENT_DESKTOP_BINARY', ROOT / 'target/debug/linnstrument-sim')).resolve()
ELEMENT = 'element-6066-11e4-a52e-4f735466cecf'


def wait_for(query, predicate, label, timeout=15):
    deadline = time.monotonic() + timeout
    value = None
    while time.monotonic() < deadline:
        value = query()
        if predicate(value):
            return value
        time.sleep(.05)
    if isinstance(value, dict) and 'host' in value:
        value = {key: value[key] for key in ['connected', 'error', 'contacts', 'paused']} | {
            'performance': value['host']['performance']}
    raise AssertionError(f'{label}: timed out; last value: {value}')


class Desktop:
    def __init__(self):
        self.session = None
        self.observations = {}

    def request(self, method, path, body=None):
        data = None if body is None else json.dumps(body).encode()
        request = urllib.request.Request('http://127.0.0.1:4444' + path, data=data,
                                        headers={'Content-Type': 'application/json'}, method=method)
        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                return json.load(response)['value']
        except urllib.error.HTTPError as error:
            raise RuntimeError(f'{method} {path}: {error.read().decode()}') from error

    def command(self, method, path, body=None):
        return self.request(method, f'/session/{self.session}{path}', body)

    def start(self):
        def ready():
            try:
                return self.request('GET', '/status')
            except (OSError, RuntimeError):
                return None
        wait_for(ready, lambda status: status and status.get('ready'), 'WebDriver startup')
        result = self.request('POST', '/session', {'capabilities': {'alwaysMatch': {
            'webkitgtk:browserOptions': {'binary': str(BINARY)}
        }}})
        self.session = result['sessionId']
        self.command('POST', '/timeouts', {'implicit': 5000, 'script': 5000, 'pageLoad': 15000})
        handles = wait_for(lambda: self.command('GET', '/window/handles'),
                           lambda handles: len(handles) == 2, 'both application windows')
        self.windows = {}
        for handle in handles:
            self.command('POST', '/window', {'handle': handle})
            title = self.command('GET', '/title')
            self.windows['instrument' if 'firmware replica' in title else 'host'] = handle
        assert len(self.windows) == 2, self.windows
        self.native_windows = {}
        for name, title in [('instrument', '^LinnStrument'), ('host', '^Crest Performance')]:
            windows = self.native('search', '--onlyvisible', '--name', title).splitlines()
            assert len(windows) == 1, (name, windows)
            self.native_windows[name] = windows[0]

    def switch(self, name):
        self.command('POST', '/window', {'handle': self.windows[name]})
        self.native('windowraise', self.native_windows[name],
                    'windowfocus', '--sync', self.native_windows[name])
        self.state(lambda s: s['connected'], 'firmware connected')

    def watch_input(self):
        self.observe('''window.desktopInputTrace=[];
            for(const type of ['keydown','keyup','blur','focus'])
                window.addEventListener(type,event=>{
                    if(window.desktopInputTrace.length===128)window.desktopInputTrace.shift();
                    window.desktopInputTrace.push({type:event.type,key:event.key,code:event.code,
                        repeat:event.repeat,target:event.target.id,focus:document.hasFocus()});
                },true);''')

    def observe(self, script, *args):
        return self.command('POST', '/execute/sync', {'script': script, 'args': list(args)})

    def element(self, selector):
        return self.command('POST', '/element', {'using': 'css selector', 'value': selector})[ELEMENT]

    def click(self, selector):
        self.command('POST', f'/element/{self.element(selector)}/click', {})

    def fill(self, selector, value):
        element = self.element(selector)
        self.command('POST', f'/element/{element}/clear', {})
        self.command('POST', f'/element/{element}/value', {'text': value})

    def native(self, *arguments):
        return subprocess.check_output(['xdotool', *arguments], text=True, timeout=10).strip()

    def press(self, key):
        self.native('key', key)

    def touch(self, x, y):
        # Read canvas coordinates; WebDriver delivers the actual mouse events.
        point = self.observe('''const r=document.querySelector('#surface').getBoundingClientRect();
            const columns=latest.firmware.model===128?17:26;
            return [Math.round(r.left+(arguments[0]+.5)*r.width/columns),
                    Math.round(r.top+(7.5-arguments[1])*r.height/8)];''', x, y)
        self.command('POST', '/actions', {'actions': [{
            'type': 'pointer', 'id': 'mouse', 'parameters': {'pointerType': 'mouse'}, 'actions': [
                {'type': 'pointerMove', 'duration': 0, 'origin': 'viewport', 'x': point[0], 'y': point[1]},
                {'type': 'pointerDown', 'button': 0},
                {'type': 'pause', 'duration': 200},
                {'type': 'pointerUp', 'button': 0},
            ]
        }]})

    def state(self, predicate, label):
        return wait_for(lambda: self.observe('return typeof latest === "undefined" ? null : latest'),
                        lambda value: value and not value['error'] and predicate(value), label)

    def capture(self, name, state=None):
        self.observations[name] = state or self.observe('return latest')
        png = self.command('GET', '/screenshot')
        (ARTIFACTS / f'{name}.png').write_bytes(base64.b64decode(png))

    def finish(self):
        (ARTIFACTS / 'observations.json').write_text(json.dumps(self.observations, indent=2) + '\n')
        if self.session:
            self.command('DELETE', '', None)


def record_expressive_keyboard_phrase(app):
    app.switch('host')
    app.click('#record')
    app.state(lambda s: s['host']['performance']['slots'][0] == 'recording', 'record armed')
    app.switch('instrument')
    app.watch_input()
    app.click('#release')  # Stable button focus accepts musical keys.
    before = app.observe('return latest.firmware.sensor_reads')
    # WebKit's automation drops chord key-ups even within one /actions request.
    # XTEST sends real OS keyboard events and preserves simultaneous held keys.
    app.native('keydown', 'a', 'f', 'k')
    app.state(lambda s: len(s['contacts']) == 3 and s['host']['performance']['active_voices'] == 3,
              'three held keyboard notes')
    # Independent selected finger: X, Y and explicit pressure, through sensors.
    for key in ['Right', 'Right', 'Up', 'Up', 'bracketright', 'bracketright', 'bracketright']:
        app.press(key)
    state = app.state(lambda s: s['contacts'][-1]['z'] > .58, 'selected finger expression')
    assert state['host']['performance']['active_voices'] == 3, state
    moving = state['contacts'][-1]
    assert moving['x'] > 10.15 and moving['y'] > 2.15 and moving['z'] > .58, moving
    assert state['firmware']['sensor_reads'] > before
    app.observations['three-finger-expression'] = state
    app.native('keyup', 'a', 'f', 'k')
    app.state(lambda s: not s['contacts'] and s['host']['performance']['active_voices'] == 0,
              'keyboard release')
    app.capture('keyboard-released')
    app.switch('host')
    app.click('#finish')
    state = app.state(lambda s: s['host']['performance']['slots'][0] == 'recorded', 'record finished')
    clip = state['host']['clips'][0]
    attacks = [e for e in clip['events'] if e['gesture']['type'] == 'on']
    assert sorted(e['gesture']['key'] for e in attacks) == [42, 45, 49], attacks
    moving = next(e['note'] for e in attacks if e['gesture']['key'] == 49)
    expression = [e['gesture']['expression'] for e in clip['events']
                  if e['note'] == moving and e['gesture']['type'] == 'expression']
    assert max(e['pitch'] for e in expression) > 100_000, expression
    assert max(e['timbre'] for e in expression) > 40_000, expression
    path = ARTIFACTS / 'desktop-session.json'
    app.fill('#path', str(path))
    app.click('#save')
    wait_for(path.exists, bool, 'saved host document')
    document = json.loads(path.read_text())
    assert document['session']['slots'][0]['clip'] == clip
    captured = state['host']['capture_count']
    app.click('#open')
    app.state(lambda s: s['host']['clips'][0] == clip and s['host']['capture_count'] < captured,
              'reopened expressive phrase')
    app.capture('recorded-phrase')


def launch_with_firmware_pad(app):
    app.click('#clips')
    app.state(lambda s: s['host']['performance']['mode'] == 'clips', 'user mode')
    app.switch('instrument')
    app.touch(1, 0)
    state = app.state(lambda s: s['host']['performance']['slots'][0] == 'playing' and
                      s['firmware']['leds'][8][1] > 200, 'pad launch and firmware green LED')
    red, green, blue = state['firmware']['leds'][8]
    assert green > red and green > blue
    # Center pixels must show the decoded firmware frame, not only an intent snapshot.
    pixel = app.observe("return Array.from(document.querySelector('#surface').getContext('2d').getImageData(63,315,1,1).data)")
    assert pixel[1] > 200 and pixel[1] > pixel[0] and pixel[1] > pixel[2], pixel
    app.capture('firmware-pad-playing', state)
    app.touch(1, 0)
    app.state(lambda s: s['host']['performance']['slots'][0] == 'recorded' and
              s['host']['performance']['active_voices'] == 0, 'pad stop cleanup')


def focus_recovery(app):
    app.switch('host')
    app.click('#play')
    app.state(lambda s: s['host']['performance']['mode'] == 'play', 'stock MPE restored')
    app.switch('instrument')
    app.click('#release')
    app.native('keydown', 'a')
    app.state(lambda s: len(s['contacts']) == 1 and s['host']['performance']['active_voices'] == 1,
              'momentary keyboard voice')
    app.switch('host')
    app.click('#stop')
    app.switch('instrument')
    app.state(lambda s: not s['contacts'] and s['host']['performance']['active_voices'] == 0,
              'focus loss releases momentary contact')
    app.native('keyup', 'a')
    app.capture('focus-recovered')


def record_timed_strum(app):
    app.switch('host')
    app.click('[name="articulation"] option[value="descending"]')
    app.click('[name="strum_interval"] option[value="120000"]')
    app.click('#harmony-form button[type="submit"]')
    settings = {'type': 'strum', 'direction': 'descending', 'interval': 120000}
    app.state(lambda s: s['host']['performance']['harmony']['articulation'] == settings,
              'strum settings accepted')
    app.click('#chords')
    app.click('#record')
    app.state(lambda s: s['host']['performance']['slots'][0] == 'recording', 'strum record armed')
    app.switch('instrument')
    app.click('#latch')
    app.touch(1, 0)
    state = app.state(lambda s: len(s['contacts']) == 1 and
                      s['host']['performance']['active_voices'] == 3, 'one trigger owns three voices')
    start = state['host']['performance']['at']
    app.press('bracketright')
    app.state(lambda s: s['host']['performance']['at'] >= start + 300000, 'native strum clock')
    app.click('#release')
    app.state(lambda s: not s['contacts'] and s['host']['performance']['active_voices'] == 0,
              'strum release cleanup')
    app.click('#latch')
    app.switch('host')
    app.click('#finish')
    state = app.state(lambda s: s['host']['performance']['slots'][0] == 'recorded', 'strum stored')
    clip = state['host']['clips'][0]
    attacks = [event for event in clip['events'] if event['gesture']['type'] == 'on']
    assert [event['gesture']['key'] for event in attacks] == [67, 64, 60], attacks
    first = attacks[0]['at']
    assert [event['at'] - first for event in attacks] == [0, 120000, 240000], attacks
    assert len([event for event in clip['events'] if event['gesture']['type'] == 'off']) == 3
    path = ARTIFACTS / 'desktop-strum-session.json'
    app.fill('#path', str(path))
    app.click('#save')
    wait_for(path.exists, bool, 'saved strum session')
    app.click('#open')
    app.state(lambda s: s['host']['clips'][0] == clip and
              s['host']['performance']['harmony']['articulation'] == settings,
              'strum settings and rendered notes reopened')
    app.capture('timed-strum')


def capture_held_phrase(app):
    app.click('#play')
    state = app.state(lambda s: s['host']['performance']['mode'] == 'play', 'ordinary playing')
    previous = state['host']['clips'][0]
    app.switch('instrument')
    app.click('#latch')
    app.touch(2, 0)
    state = app.state(lambda s: s['host']['performance']['active_voices'] == 1, 'held capture source')
    start = state['host']['performance']['at']
    app.press('bracketright')
    app.state(lambda s: s['host']['performance']['at'] > start + 3840000,
              'held source covers four-beat history window')
    app.switch('host')
    app.click('#retro')
    state = app.state(lambda s: s['host']['clips'][0] != previous, 'retrospective clip captured')
    clip = state['host']['clips'][0]
    assert state['host']['performance']['active_voices'] == 1
    attacks = [event for event in clip['events'] if event['gesture']['type'] == 'on']
    assert len(attacks) == 1 and attacks[0]['gesture']['key'] == 31 and attacks[0]['at'] == 0
    assert clip['length'] == 3840000
    assert clip['events'][-1]['gesture']['type'] == 'off'
    assert clip['events'][-1]['at'] == clip['length']
    app.capture('retrospective-held-note', state)
    app.click('#undo')
    app.state(lambda s: s['host']['clips'][0] == previous and
              s['host']['performance']['active_voices'] == 1, 'undo preserves original held input')
    app.switch('instrument')
    app.click('#release')
    app.click('#latch')
    app.state(lambda s: not s['contacts'] and s['host']['performance']['active_voices'] == 0,
              'original captured input releases normally')


def learn_played_voicing(app):
    app.switch('instrument')
    app.click('#latch')
    for x in [2, 6, 9]:
        app.touch(x, 2)
    app.state(lambda s: s['host']['performance']['active_voices'] == 3, 'three held source notes')
    app.switch('host')
    app.click('#degrees button:first-child')
    app.click('#learn-chord')
    learned = {'notes': [41, 45, 48], 'root': 41}
    app.state(lambda s: s['host']['performance']['harmony']['learned'][0] == learned,
              'played voicing learned through public command')
    app.switch('instrument')
    app.click('#release')
    app.click('#latch')
    app.state(lambda s: s['host']['performance']['active_voices'] == 0, 'learned source released')
    app.switch('host')
    app.click('[name="articulation"] option[value="block"]')
    app.click('#harmony-form button[type="submit"]')
    app.click('#chords')
    app.state(lambda s: s['host']['performance']['mode'] == 'chords', 'learned chord playing mode')
    app.click('#record')
    app.state(lambda s: s['host']['performance']['slots'][0] == 'recording', 'learned chord recording')
    app.switch('instrument')
    app.click('#latch')
    app.touch(1, 0)
    app.state(lambda s: s['host']['performance']['active_voices'] == 3, 'one firmware finger plays learned voicing')
    app.press('bracketright')
    app.click('#release')
    app.click('#latch')
    app.state(lambda s: s['host']['performance']['active_voices'] == 0, 'learned voices release')
    app.switch('host')
    app.click('#finish')
    state = app.state(lambda s: s['host']['performance']['slots'][0] == 'recorded', 'learned take committed')
    clip = state['host']['clips'][0]
    pitches = [e['gesture']['key'] for e in clip['events'] if e['gesture']['type'] == 'on']
    assert pitches == [41, 45, 48], pitches
    path = ARTIFACTS / 'desktop-learned-chord.json'
    app.fill('#path', str(path))
    app.click('#save')
    wait_for(path.exists, bool, 'saved learned bank')
    app.click('#clear-chord')
    app.state(lambda s: s['host']['performance']['harmony']['learned'][0] is None, 'bank entry cleared')
    app.click('#undo')
    app.state(lambda s: s['host']['performance']['harmony']['learned'][0] == learned, 'bank Undo')
    app.click('#open')
    app.state(lambda s: s['host']['performance']['harmony']['learned'][0] == learned and
              s['host']['clips'][0] == clip, 'bank and rendered performance reopened')
    app.capture('learned-chord-bank')


def record_repeating_arpeggio(app):
    app.switch('host')
    app.click('[name="articulation"] option[value="arpeggio_ascending"]')
    app.click('[name="strum_interval"] option[value="240000"]')
    app.click('[name="arpeggio_gate"] option[value="120000"]')
    app.click('#harmony-form button[type="submit"]')
    settings = {'type': 'arpeggio', 'direction': 'ascending', 'interval': 240000, 'gate': 120000}
    app.state(lambda s: s['host']['performance']['harmony']['articulation'] == settings,
              'arpeggio settings accepted')
    app.click('#chords')
    app.click('#record')
    app.state(lambda s: s['host']['performance']['slots'][0] == 'recording', 'arpeggio record armed')
    app.switch('instrument')
    app.click('#latch')
    app.touch(1, 0)
    state = app.state(lambda s: s['host']['performance']['active_voices'] == 3,
                      'one firmware finger owns repeating learned chord')
    start = state['host']['performance']['at']
    app.press('Right')
    app.press('Up')
    app.press('bracketright')
    app.state(lambda s: s['host']['performance']['at'] >= start + 1100000,
              'native clock reaches second arpeggio cycle')
    app.click('#release')
    app.click('#latch')
    app.state(lambda s: not s['contacts'] and s['host']['performance']['active_voices'] == 0,
              'arpeggio trigger release cancels future notes')
    app.switch('host')
    app.click('#finish')
    state = app.state(lambda s: s['host']['performance']['slots'][0] == 'recorded', 'arpeggio stored')
    clip = state['host']['clips'][0]
    verify_desktop_arpeggio(clip)
    path = ARTIFACTS / 'desktop-arpeggio-session.json'
    app.fill('#path', str(path))
    app.click('#save')
    wait_for(path.exists, bool, 'saved arpeggio session')
    app.click('#open')
    app.state(lambda s: s['host']['clips'][0] == clip and
              s['host']['performance']['harmony']['articulation'] == settings,
              'arpeggio settings and exact rendered notes reopened')
    app.capture('repeating-arpeggio')


def verify_desktop_arpeggio(clip):
    attacks = [e for e in clip['events'] if e['gesture']['type'] == 'on']
    assert len(attacks) >= 4, attacks
    start = attacks[0]['at']
    assert [e['gesture']['key'] for e in attacks] == [[41, 45, 48][i % 3] for i in range(len(attacks))]
    assert [e['at'] - start for e in attacks] == [i * 240000 for i in range(len(attacks))]
    releases = {json.dumps(e['note'], sort_keys=True): e['at'] for e in clip['events'] if e['gesture']['type'] == 'off'}
    assert len(releases) == len(attacks)
    for index, attack in enumerate(attacks):
        duration = releases[json.dumps(attack['note'], sort_keys=True)] - attack['at']
        expected = 120000 if index + 1 < len(attacks) else duration
        assert duration == expected and 0 <= duration <= 120000
    expressions = [e['gesture']['expression'] for e in clip['events'] if 'expression' in e['gesture']]
    for dimension in ['pitch', 'timbre', 'pressure']:
        assert expression_varies(expressions, dimension), dimension


def expression_varies(expressions, dimension):
    return len({expression[dimension] for expression in expressions}) > 1


def edit_conditions_and_hold_fill(app):
    app.switch('host')
    app.click('#stop')
    app.click('#grid button:nth-child(2)')
    app.click('#pattern-panel summary')
    for field, value in [('steps', '1'), ('pulses', '1'), ('pitch_steps', '1'),
                         ('tone_min', '0'), ('tone_max', '0')]:
        app.fill(f'#pattern-form [name="{field}"]', value)
    app.click('#pattern-form button[type="submit"]')
    state = app.state(lambda s: s['host']['performance']['slots'][1] == 'recorded',
                      'short pattern for condition editing')
    original = state['host']['clips'][1]
    app.click('#next-note')
    app.click('#condition-form [name="condition"] option[value="every"]')
    app.click('#condition-form button[type="submit"]')
    state = app.state(lambda s: s['host']['clips'][1].get('conditions'), 'every fourth loop edit')
    assert state['host']['clips'][1]['conditions'][0]['condition'] == {'type': 'every', 'period': 4, 'offset': 3}
    assert state['host']['clips'][1]['events'] == original['events']
    app.click('#undo')
    app.state(lambda s: s['host']['clips'][1] == original, 'condition Undo restores original')
    app.click('#condition-form [name="condition"] option[value="fill"]')
    app.click('#condition-form button[type="submit"]')
    state = app.state(lambda s: s['host']['clips'][1].get('conditions', [{}])[0].get('condition') == {'type': 'fill'},
                      'Fill attack condition')
    clip = state['host']['clips'][1]
    path = ARTIFACTS / 'desktop-conditions-session.json'
    app.fill('#path', str(path))
    app.click('#save')
    wait_for(path.exists, bool, 'condition session save')
    app.click('#open')
    app.state(lambda s: s['host']['clips'][1] == clip and not s['host']['performance']['fill'],
              'conditions reopened with Fill released')
    app.click('#grid button:nth-child(2)')
    state = app.state(lambda s: s['host']['performance']['slots'][1] == 'playing', 'conditional loop launched')
    start, count = state['host']['performance']['at'], state['host']['capture_count']
    state = app.state(lambda s: s['host']['performance']['at'] > start + 960000, 'silent loops before Fill')
    assert state['host']['capture_count'] == count
    # A real keyboard hold owns Fill; browser repeat must not retrigger it.
    app.click('#fill')
    app.native('keydown', 'space')
    state = app.state(lambda s: s['host']['performance']['fill'], 'keyboard Fill held')
    start = state['host']['performance']['at']
    app.state(lambda s: s['host']['performance']['at'] > start + 960000 and
              s['host']['capture_count'] > count, 'held Fill admits repeated attacks')
    app.native('keyup', 'space')
    state = app.state(lambda s: not s['host']['performance']['fill'], 'Fill release')
    start = state['host']['performance']['at']
    state = app.state(lambda s: s['host']['performance']['at'] > start + 480000 and
                      s['host']['performance']['active_voices'] == 0, 'accepted notes release normally')
    start, count = state['host']['performance']['at'], state['host']['capture_count']
    state = app.state(lambda s: s['host']['performance']['at'] > start + 960000, 'released Fill stays silent')
    assert state['host']['capture_count'] == count
    app.native('keydown', 'space')
    app.state(lambda s: s['host']['performance']['fill'], 'second Fill hold')
    app.switch('instrument')
    app.state(lambda s: not s['host']['performance']['fill'], 'focus loss releases Fill')
    app.native('keyup', 'space')
    app.switch('host')
    app.click('#stop')
    app.state(lambda s: s['host']['performance']['active_voices'] == 0, 'conditional playback stopped')
    app.capture('conditional-notes-and-fill')


def record_humanized_chord(app):
    app.switch('host')
    app.click('#grid button:first-child')
    app.click('[name="articulation"] option[value="humanized"]')
    app.click('#harmony-form button[type="submit"]')
    settings = {'type': 'humanized', 'max_delay': 60000, 'velocity_variation': 8, 'seed': 42}
    app.state(lambda s: s['host']['performance']['harmony']['articulation'] == settings, 'humanized articulation accepted')
    app.click('#chords')
    app.click('#record')
    app.state(lambda s: s['host']['performance']['slots'][0] == 'recording', 'humanized recording armed')
    app.switch('instrument')
    app.click('#latch')
    app.touch(1, 0)
    app.state(lambda s: s['host']['performance']['active_voices'] == 3, 'humanized chord owned by one finger')
    app.press('Right')
    app.press('Up')
    app.press('bracketright')
    app.click('#release')
    app.click('#latch')
    app.state(lambda s: s['host']['performance']['active_voices'] == 0, 'humanized chord release')
    app.switch('host')
    app.click('#finish')
    state = app.state(lambda s: s['host']['performance']['slots'][0] == 'recorded', 'humanized clip committed')
    clip = state['host']['clips'][0]
    triggers = [o['input'] for o in clip['harmony_origin'] if o['input']['gesture']['type'] == 'on']
    assert len(triggers) == 1 and triggers[0]['note'] == {'source': 1, 'track': 1, 'note': 1}, triggers
    trigger = triggers[0]
    attacks = {e['gesture']['key']: e for e in clip['events'] if e['gesture']['type'] == 'on'}
    assert set(attacks) == {41, 45, 48}
    for key, delay, delta in [(41, 49021, 5), (45, 35462, 3), (48, 32129, 1)]:
        assert attacks[key]['at'] == trigger['at'] + delay
        assert attacks[key]['gesture']['velocity'] == min(127, trigger['gesture']['velocity'] + delta)
    expressions = [e['gesture']['expression'] for e in clip['events'] if 'expression' in e['gesture']]
    for dimension in ['pitch', 'timbre', 'pressure']:
        assert expression_varies(expressions, dimension), dimension
    path = ARTIFACTS / 'desktop-humanized-session.json'
    app.fill('#path', str(path))
    app.click('#save')
    wait_for(path.exists, bool, 'humanized session saved')
    app.click('#open')
    app.state(lambda s: s['host']['clips'][0] == clip and s['host']['performance']['harmony']['articulation'] == settings,
              'humanized clip and settings reopened exactly')
    app.capture('humanized-chord')


def play_showcase_bank(app):
    app.switch('host')
    packaged = BINARY.parent / 'examples/performance-showcase.json'
    path = packaged if packaged.is_file() else ROOT / 'examples/performance-showcase.json'
    app.fill('#path', str(path))
    app.click('#open')
    state = app.state(lambda s: len(s['host']['tracks']) == 3 and
                      s['host']['performance']['slots'][:3] == ['recorded'] * 3,
                      'showcase bank reopened')
    assert [route['destination'] for route in state['host']['outputs']] == [1, 2, 3]
    app.click('#clips')
    app.state(lambda s: s['host']['performance']['mode'] == 'clips', 'showcase clip pad mode')
    app.switch('instrument')
    for column in [1, 2, 3]:
        app.touch(column, 0)
    state = app.state(lambda s: s['host']['performance']['slots'][:3] == ['playing'] * 3 and
                      all(s['firmware']['leds'][column * 8][1] > 200 for column in [1, 2, 3]),
                      'three independent clips launched by firmware pads')
    assert not any(m['destination'] == 3 for m in state['host']['recent_midi'])
    app.capture('showcase-three-clips', state)
    app.switch('host')
    app.watch_input()
    app.click('#fill')
    app.native('keydown', 'space')
    app.state(lambda s: s['host']['performance']['fill'], 'showcase keyboard Fill held')
    app.state(lambda s: s['host']['performance']['fill'] and any(
        m['destination'] == 3 and m['message']['bytes'][0] & 0xf0 == 0x90
        for m in s['host']['recent_midi']), 'showcase Fill reaches independent destination')
    app.native('keyup', 'space')
    app.state(lambda s: not s['host']['performance']['fill'], 'showcase Fill released')
    app.switch('instrument')
    app.touch(2, 0)
    state = app.state(lambda s: s['host']['performance']['slots'][:3] == ['playing', 'recorded', 'playing'] and
                      s['firmware']['leds'][16][2] > 200 and
                      s['firmware']['leds'][8][1] > 200 and s['firmware']['leds'][24][1] > 200,
                      'stopping bass preserves other clips and independent firmware LEDs')
    app.capture('showcase-independent-stop', state)
    app.switch('host')
    app.click('#stop')
    app.click('#play')
    app.state(lambda s: s['host']['performance']['active_voices'] == 0 and
              s['host']['performance']['slots'][:3] == ['recorded'] * 3 and
              s['host']['performance']['mode'] == 'play', 'showcase restored stock playing mode')



def main():
    ARTIFACTS.mkdir(parents=True, exist_ok=True)
    app = Desktop()
    with (ARTIFACTS / 'webdriver.log').open('w') as log:
        # Linux's native driver owns the protocol. Tauri's proxy only maps this
        # capability and environment flag; omitting it also avoids its pooled
        # HTTP connection-reset failure between otherwise valid commands.
        driver = subprocess.Popen(['WebKitWebDriver', '--port=4444'], cwd=BINARY.parent,
                                  env={**os.environ, 'TAURI_WEBVIEW_AUTOMATION': 'true', 'GDK_BACKEND': 'x11'},
                                  stdout=log, stderr=subprocess.STDOUT)
        try:
            app.start()
            record_expressive_keyboard_phrase(app)
            launch_with_firmware_pad(app)
            focus_recovery(app)
            record_timed_strum(app)
            capture_held_phrase(app)
            learn_played_voicing(app)
            record_repeating_arpeggio(app)
            edit_conditions_and_hold_fill(app)
            record_humanized_chord(app)
            play_showcase_bank(app)
            print('Desktop PASS: keyboard XYZ, record/reopen, mouse pad launch/stop, firmware LEDs, focus recovery, timed strum, retrospective capture, learned chord bank, repeating arpeggio, conditions and Fill, humanized chord, three-route showcase')
        except Exception:
            if app.session:
                events = app.observe('return window.desktopInputTrace || []')
                app.observations['input-events'] = events
                print('Native input events:', json.dumps(events), flush=True)
                app.capture('failure')
            raise
        finally:
            try:
                app.finish()
            finally:
                driver.terminate()
                driver.wait(timeout=10)


if __name__ == '__main__':
    main()
