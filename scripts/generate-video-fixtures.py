"""Generate deterministic CFR, VFR and rotated MP4 fixtures using user/local FFmpeg."""
import shutil
import os,sys,json,hashlib,subprocess,zlib,struct
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'tests/fixtures/video';OUT.mkdir(parents=True,exist_ok=True)
ffmpeg=os.environ.get('LABELFLOW_FFMPEG') or shutil.which('ffmpeg') or str(ROOT/'.tools/ffmpeg/ffmpeg.exe')
ffprobe=os.environ.get('LABELFLOW_FFPROBE') or shutil.which('ffprobe') or str(ROOT/'.tools/ffmpeg/ffprobe.exe')
def run(args): return subprocess.check_output(args,stderr=subprocess.PIPE)
def png(w,h,pixels):
 def chunk(t,d):return struct.pack('>I',len(d))+t+d+struct.pack('>I',zlib.crc32(t+d)&0xffffffff)
 raw=b''.join(b'\0'+pixels[y*w*3:(y+1)*w*3] for y in range(h))
 return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',w,h,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(raw))+chunk(b'IEND',b'')
frames=ROOT/'.cache/m2-video-frames';frames.mkdir(exist_ok=True)
for i in range(12):
 pixels=bytes(v for y in range(48) for x in range(64) for v in ((i*19+x*2)%256,(i*11+y*4)%256,(i*7+x+y)%256))
 (frames/f'{i:02}.png').write_bytes(png(64,48,pixels))
run([ffmpeg,'-v','error','-y','-framerate','10','-i',str(frames/'%02d.png'),'-c:v','libx264','-pix_fmt','yuv420p','-bf','2',str(OUT/'cfr.mp4')])
concat=frames/'vfr.ffconcat';concat.write_text('ffconcat version 1.0\n'+''.join(f"file '{i:02}.png'\nduration {d}\n" for i,d in enumerate([.04,.12,.08,.2,.04,.16,.28,.08,.12,.04,.24,.12]))+"file '11.png'\n",encoding='utf-8')
run([ffmpeg,'-v','error','-y','-safe','0','-f','concat','-i',str(concat),'-fps_mode','vfr','-c:v','libx264','-pix_fmt','yuv420p','-bf','2',str(OUT/'vfr.mp4')])
run([ffmpeg,'-v','error','-y','-display_rotation','90','-i',str(OUT/'cfr.mp4'),'-c','copy',str(OUT/'rotated.mp4')])
metadata={}
for name in ['cfr.mp4','vfr.mp4','rotated.mp4']:
 source=OUT/name
 probe=json.loads(run([ffprobe,'-v','error','-select_streams','v:0','-show_streams','-show_frames','-show_entries','stream=width,height,time_base:frame=best_effort_timestamp,best_effort_timestamp_time','-of','json',str(source)]))
 metadata[name]={'sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'probe':probe}
(OUT/'expected.json').write_text(json.dumps(metadata,indent=2)+'\n',encoding='utf-8',newline='\n')
print('Generated 3 MP4 fixtures, including VFR, B frames and rotation.')
