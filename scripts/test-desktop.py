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
        value = {key: value[key] for key in ['connected', 'error', 'contacts', 'paused']}
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
            print('Desktop PASS: keyboard XYZ, record/reopen, mouse pad launch/stop, firmware LEDs, focus recovery, timed strum')
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
