#!/usr/bin/env python3
"""Bundle a relocatable native development build, with provenance and notices."""
import hashlib
import json
import pathlib
import platform
import shutil
import subprocess
import tarfile
import tempfile
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[1]


def command(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def copy_notice_files(package, directory):
    source = pathlib.Path(package['manifest_path']).parent
    notices = {p for p in source.iterdir()
               if p.name.upper().startswith(('LICENSE', 'LICENCE', 'COPYING', 'NOTICE', 'UNLICENSE'))}
    if package.get('license_file'):
        notices.add(source / package['license_file'])
    for path in sorted(notices):
        directory.mkdir(parents=True, exist_ok=True)
        if path.is_dir():
            shutil.copytree(path, directory / path.name, dirs_exist_ok=True)
        else:
            shutil.copy2(path, directory / path.name)


def dependency_notices(stage):
    metadata = json.loads(command('cargo', 'metadata', '--format-version', '1', '--locked',
                                  '--features', 'linnstrument-sim/desktop,crest-performance-host/midi'))
    dependencies = []
    for package in sorted(metadata['packages'], key=lambda p: (p['name'], p['version'])):
        if package['source'] is None:
            continue
        dependencies.append({key: package[key] for key in ['name', 'version', 'source', 'license']})
        copy_notice_files(package, stage / 'licenses' / f"{package['name']}-{package['version']}")
    (stage / 'dependencies.json').write_text(json.dumps(dependencies, indent=2) + '\n')


def populate(stage, version, revision):
    stage.mkdir()
    for name in ['crest-performance-host', 'linnstrument-sim', 'linnstrument-replay']:
        shutil.copy2(ROOT / 'target/release' / name, stage / name)
    assets = ROOT / 'target/firmware-native'
    shutil.copy2(assets / 'linnstrument-worker', stage / 'linnstrument-worker')
    for path in sorted(assets.glob('ideal-*')):
        if path.suffix in ['.bin', '.json']:
            shutil.copy2(path, stage / path.name)
    for name in ['LICENSE', 'README.md', 'Cargo.lock']:
        shutil.copy2(ROOT / name, stage / name)
    for name in ['docs', 'examples']:
        shutil.copytree(ROOT / name, stage / name)
    shutil.copytree(ROOT / 'crates/linnstrument-sim/scenarios', stage / 'scenarios')
    dependency_notices(stage)
    firmware = ROOT / 'third_party/linnstrument-firmware'
    shutil.copy2(firmware / 'LICENSE.txt', stage / 'licenses/linnstrument-firmware.txt')
    shutil.copy2(firmware / 'libraries/DueFlashStorage/licence (MIT).txt',
                 stage / 'licenses/DueFlashStorage.txt')
    build = {'package_version': version, 'repository_commit': revision,
             'development_candidate': True, 'os': platform.system(), 'machine': platform.machine(),
             'rustc': command('rustc', '--version'), 'lock_sha256': sha256(ROOT / 'Cargo.lock'),
             'firmware': json.loads((assets / 'ideal-200.json').read_text())}
    (stage / 'build-info.json').write_text(json.dumps(build, indent=2) + '\n')
    files = sorted(path for path in stage.rglob('*') if path.is_file())
    (stage / 'SHA256SUMS').write_text(''.join(f'{sha256(p)}  {p.relative_to(stage)}\n' for p in files))


def main():
    version = tomllib.loads((ROOT / 'Cargo.toml').read_text())['workspace']['package']['version']
    revision = command('git', 'rev-parse', 'HEAD')
    if command('git', 'status', '--porcelain', '--untracked-files=no'):
        raise SystemExit('Commit tracked changes before packaging: provenance must identify the build.')
    name = f'crest-performance-{version}-{platform.system().lower()}-{platform.machine()}-{revision[:12]}'
    output = ROOT / 'target/candidate'
    output.mkdir(parents=True, exist_ok=True)
    archive = output / f'{name}.tar.gz'
    with tempfile.TemporaryDirectory(prefix='crest-package-') as temporary:
        stage = pathlib.Path(temporary) / name
        populate(stage, version, revision)
        with tarfile.open(archive, 'w:gz') as bundle:
            bundle.add(stage, arcname=name)
    archive.with_suffix(archive.suffix + '.sha256').write_text(f'{sha256(archive)}  {archive.name}\n')
    print(archive)


if __name__ == '__main__':
    main()
