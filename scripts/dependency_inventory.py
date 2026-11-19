"""Record license metadata for installed dependencies; no external service required."""
import json
import os
from pathlib import Path
import tomllib

ROOT = Path(__file__).resolve().parent.parent


def inventory():
    lock = json.loads((ROOT / 'package-lock.json').read_text(encoding='utf-8'))
    npm = []
    for path, package in lock['packages'].items():
        if not path:
            continue
        manifest = ROOT / path / 'package.json'
        if not manifest.exists():
            continue  # Optional binaries for other operating systems aren't installed.
        value = json.loads(manifest.read_text(encoding='utf-8'))
        npm.append({'name': value['name'], 'version': value['version'], 'license': value.get('license', 'UNSPECIFIED'), 'dev': package.get('dev', False)})
    cargo_home = Path(os.environ.get('CARGO_HOME', ROOT / '.tools/cargo'))
    registries = list((cargo_home / 'registry/src').glob('*'))
    cargo = []
    rust_lock = tomllib.loads((ROOT / 'src-tauri/Cargo.lock').read_text(encoding='utf-8'))
    unavailable = []
    for package in rust_lock['package']:
        if 'source' not in package:
            continue
        candidates = [registry / f"{package['name']}-{package['version']}" / 'Cargo.toml' for registry in registries]
        manifest = next((p for p in candidates if p.exists()), None)
        if manifest is None:
            unavailable.append(f"{package['name']}@{package['version']}")
            continue
        value = tomllib.loads(manifest.read_text(encoding='utf-8'))['package']
        cargo.append({'name': value['name'], 'version': value['version'], 'license': value.get('license', 'UNSPECIFIED'), 'licenseFile': value.get('license-file')})
    return {'scope': 'Installed npm packages and locally fetched Cargo sources; other-target crates listed separately.', 'npm': sorted(npm, key=lambda p: p['name']), 'cargo': sorted(cargo, key=lambda p: p['name']), 'unfetchedCargoPackages': unavailable}


if __name__ == '__main__':
    result = inventory()
    target = ROOT / 'docs/architecture/dependency-inventory.json'
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    print(f"Inventoried {len(result['npm'])} npm and {len(result['cargo'])} Cargo packages; {len(result['unfetchedCargoPackages'])} other/unfetched Cargo packages.")
