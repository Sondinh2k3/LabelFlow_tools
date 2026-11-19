"""Optional test dependency fetch. Nothing here is imported or shipped by LabelFlow."""
import hashlib,json,urllib.request
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
COMMIT='3cd02542899c1bcab5690b02033bc18aa07df0d4'
TARGET=ROOT/'.cache/validation';TARGET.mkdir(parents=True,exist_ok=True)
meta={'commit':COMMIT,'scope':'test-only AST loader; not product code'}
for name,remote in [('data-utils','data/utils.py'),('ops','utils/ops.py')]:
 url=f'https://raw.githubusercontent.com/ultralytics/ultralytics/{COMMIT}/ultralytics/{remote}'
 source=urllib.request.urlopen(url,timeout=60).read()
 (TARGET/f'ultralytics-{name}.py').write_bytes(source)
 digest=hashlib.sha256(source).hexdigest()
 expected={'data-utils':'cb82d4d61e652ffc5cff609826a363129ff9b8e114a4ec0e4a9fb1b0b0556f12','ops':'988a3b5491df5d2901514b9a1efd20fc20ea9a2d5fa8c9f02c3aa1c6e1692c5f'}
 assert digest==expected[name], 'Pinned parser source checksum changed'
 meta[name]={'url':url,'sha256':digest}
(TARGET/'source.json').write_text(json.dumps(meta,indent=2),encoding='utf-8')
print('Pinned test parser prepared:',COMMIT)
