"""Validate real exports with COCO API and the official Ultralytics label parser.

Run after video_acceptance. Only the selected official parser functions are executed
from the pinned downloaded AST; no torch runtime, model, training or AI is used.
"""
import shutil
import ast,os,sys,json,hashlib,sqlite3,subprocess,types
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1];sys.path.insert(0,str(ROOT/'.cache/python'))
import numpy as np
from PIL import Image,ImageOps
from pycocotools.coco import COCO
from pycocotools import mask as mask_api
import yaml
latest=json.loads((ROOT/'.cache/m2-acceptance/latest.json').read_text(encoding='utf-8'))
export=Path(latest['export']);seg_export=Path(latest['yoloSegExport']);database=sqlite3.connect(Path(latest['project'])/'.labelflow/project.sqlite')
validated_coco=validated_rle=validated_yolo=validated_pixels=0
for split in ['train','val','test']:
 coco=COCO(str(export/f'annotations/instances_{split}.json'))
 for image in coco.imgs.values():
  source=export/image['file_name'];assert Image.open(source).size==(image['width'],image['height'])
 for ann in coco.anns.values():
  assert ann['image_id'] in coco.imgs and ann['category_id'] in coco.cats
  if 'segmentation' in ann:
   decoded=coco.annToMask(ann);image=coco.imgs[ann['image_id']];assert decoded.shape==(image['height'],image['width'])
   if isinstance(ann['segmentation'],dict):
    # Both instance-mask and polygon-with-hole fixtures use exactly this raster.
    expected=np.zeros_like(decoded);expected[0:10,0:12]=1;expected[3:6,3:7]=0
    assert np.array_equal(decoded,expected),'COCO external RLE decoder disagrees pixel-for-pixel'
    assert float(mask_api.area(ann['segmentation']))==ann['area'];validated_rle+=1
  validated_coco+=1
# Use unchanged official AST functions. Torch's unvisited branch has a type marker;
# all inputs here are numpy arrays and verified PNGs from a private export directory.
namespace={'np':np,'os':os,'Image':Image,'ImageOps':ImageOps,'Path':Path,'torch':types.SimpleNamespace(Tensor=type('UnusedTensor',(),{})),'IMG_FORMATS':{'png','jpg','jpeg'},'FORMATS_HELP_MSG':'PNG/JPEG','HELP_URL':''}
metadata=json.loads((ROOT/'.cache/validation/source.json').read_text(encoding='utf-8'))
def load(name,names):
 path=ROOT/'.cache/validation'/name
 key='ops' if 'ops.py' in name else 'data-utils'
 assert hashlib.sha256(path.read_bytes()).hexdigest()==metadata[key]['sha256']
 tree=ast.parse(path.read_text(encoding='utf-8'))
 nodes=[node for node in tree.body if isinstance(node,ast.FunctionDef) and node.name in names]
 assert len(nodes)==len(names)
 exec(compile(ast.Module(body=nodes,type_ignores=[]),str(path),'exec'),namespace)
load('ultralytics-ops.py',{'empty_like','xyxy2xywh','segments2boxes'})
load('ultralytics-data-utils.py',{'exif_size','check_image','verify_image_label','img2label_paths'})
config=yaml.safe_load((seg_export/'data.yaml').read_text(encoding='utf-8'));assert len(config['names'])==5
for image in sorted((seg_export/'images').rglob('*.png')):
 label=namespace['img2label_paths']([str(image)])[0]
 result=namespace['verify_image_label']((str(image),label,'M2: ',False,5,0,0,False))
 assert result[8]==0 and result[0] is not None,result[-1]
 assert len(result[3])==len(result[1])==1
 validated_yolo+=1
# Independent full-stream decode (without select=...) checks that every extracted
# image is exactly the corresponding presentation-order frame, including rotation.
ffmpeg=os.environ.get('LABELFLOW_FFMPEG') or shutil.which('ffmpeg') or str(ROOT/'.tools/ffmpeg/ffmpeg.exe')
expected=json.loads((ROOT/'tests/fixtures/video/expected.json').read_text(encoding='utf-8'))
for item in latest['videos']:
 video=item['video'];source=Path(video['path']);assert hashlib.sha256(source.read_bytes()).hexdigest()==video['sha256']
 probe=expected[source.name]['probe'];raw_pts=[f['best_effort_timestamp'] for f in probe['frames']]
 assert [f['pts'] for f in item['frames']]==raw_pts
 for f in item['frames']:
  wanted=(f['pts']-video['startPts'])*1000*video['timeBaseNum']/video['timeBaseDen'];assert abs(f['timeMs']-wanted)<1e-7
 w,h=(video['height'],video['width']) if video['rotation']%180==90 else (video['width'],video['height'])
 raw=subprocess.check_output([ffmpeg,'-v','error','-i',str(source),'-map','0:v:0','-an','-fps_mode','passthrough','-pix_fmt','rgb24','-f','rawvideo','pipe:1'],stderr=subprocess.PIPE)
 baseline=np.frombuffer(raw,dtype=np.uint8).reshape((-1,h,w,3))
 for path,index,pts,time_ms in database.execute('SELECT path,frame_index,frame_pts,frame_time_ms FROM media WHERE source_video_id=? ORDER BY frame_index',(video['id'],)):
  assert pts==raw_pts[index]
  image=np.asarray(Image.open(Path(latest['project'])/path).convert('RGB'))
  assert np.array_equal(image,baseline[index]),f'Extracted frame {index} differs from independent sequential decode'
  validated_pixels+=1
result={'cocoAnnotations':validated_coco,'rlePixelExact':validated_rle,'yoloParserPairs':validated_yolo,'exactVideoFrames':validated_pixels,'ultralyticsSource':metadata,'pycocotools':'2.0.11','pillow':Image.__version__,'scope':'COCO API and unchanged official YOLO verification functions; no full training run'}
(ROOT/'.cache/m2-acceptance/parsers.json').write_text(json.dumps(result,indent=2),encoding='utf-8');print(json.dumps(result,indent=2))
