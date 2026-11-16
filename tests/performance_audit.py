"""Whole-app CPU/GPU audit: set ASTER_AUDIT_MEDIA to a local synthetic clip.
Python 3, ffmpeg, libmpv and a graphical desktop.
Usage: ASTER_AUDIT_MEDIA=/path/to/synthetic.mp4 python tests/performance_audit.py /absolute/path/to/aster [artifact-directory]
Creates synthetic media and a loopback mock; never touches real watch history.
"""
import os,pathlib,subprocess,socket,json,time,threading,http.server,urllib.parse,tempfile,sys,struct
root=pathlib.Path(__file__).resolve().parents[1]
binary=pathlib.Path(sys.argv[1]).resolve() if len(sys.argv)>1 else root/'target/debug/aster'
workspace=tempfile.TemporaryDirectory(prefix='aster-embedded-qa-')
r=pathlib.Path(sys.argv[2]).resolve() if len(sys.argv)>2 else pathlib.Path(workspace.name)
r.mkdir(parents=True,exist_ok=True)
server_chapters=os.environ.get('ASTER_TEST_SERVER_CHAPTERS')=='1'
chapter_names=['Server Opening','Server Main story','Server Credits'] if server_chapters else ['Opening','Main story','Credits']
media=pathlib.Path(os.environ['ASTER_AUDIT_MEDIA']).resolve()
video_bytes=media.read_bytes()
subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-i',str(media),'-frames:v','1','-y',str(r/'atlas.png')],check=True)
audit_items=[{'Id':f'preview-{i}','Name':f'Title {i:04}','Type':'Movie','Overview':'Synthetic whole-app audit. No real library or credentials.','RunTimeTicks':300000000} for i in range(360)]
(r/'layout-fixture.json').write_text(json.dumps({'items':audit_items,'atlas':str(r/'atlas.png'),'posters':[[0,0,1920,1080]],'backdrop':[0,0,1920,1080],'shelves':[{'title':f'Audit row {i}','landscape':i<2,'start':i*60,'count':60} for i in range(6)]}))
trickplay_enabled=os.environ.get('ASTER_TEST_TRICKPLAY')=='1'
trickplay_sheet=(r/'atlas.png').read_bytes();preview_requests=[]
video=video_bytes;short_video=video_bytes;reports=[]
class Handler(http.server.BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_POST(self):
  body=self.rfile.read(int(self.headers.get('Content-Length','0')))
  if self.path.split('?')[0].endswith('/PlaybackInfo'):data=json.dumps({'MediaSources':[{'Id':'source','MediaStreams':[]}]}).encode();self.send_response(200)
  elif self.path.startswith('/Sessions/'):
   reports.append((self.path,json.loads(body)));data=b'';self.send_response(204)
  else:data=b'{}';self.send_response(404)
  self.send_header('Content-Length',str(len(data)));self.send_header('Content-Type','application/json');self.end_headers();self.wfile.write(data)
 def do_GET(self):
  if self.path.startswith('/Users/') or '/Trickplay/' in self.path:
   if self.headers.get('X-Emby-Token')!='fixture-token':self.send_error(401);return
   preview_requests.append(self.path)
   if '/Trickplay/' in self.path:
    content=trickplay_sheet;kind='image/png'
   else:
    grids={'fixture-media':{'480':{'Width':480,'Height':270,'TileWidth':2,'TileHeight':2,'ThumbnailCount':4,'Interval':10000}}} if trickplay_enabled and self.path.endswith('/fixture-media') else {}
    chapters=[{'StartPositionTicks':n*100000000,'Name':title} for n,title in enumerate(['Server Opening','Server Main story','Server Credits'])] if self.path.endswith('/fixture-media') else []
    content=json.dumps({'Trickplay':grids,'Chapters':chapters}).encode();kind='application/json'
   self.send_response(200);self.send_header('Content-Type',kind);self.send_header('Content-Length',str(len(content)));self.end_headers();self.wfile.write(content);return
  if self.path.startswith('/Shows/'):
   start=dict(urllib.parse.parse_qsl(urllib.parse.urlparse(self.path).query)).get('StartItemId')
   def dto(id,number):return {'Id':id,'Name':f'Episode {number}','Type':'Episode','SeriesId':'fixture-show','SeriesName':'Fixture show','SeasonId':'fixture-season','ParentIndexNumber':1,'IndexNumber':number,'RunTimeTicks':30000000}
   items=[dto('episode-two',2)] if start=='episode-two' else [dto('episode-one',1),dto('episode-two',2)]
   content=json.dumps({'Items':items,'TotalRecordCount':len(items)}).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(content)));self.end_headers();self.wfile.write(content);return
  if not self.path.startswith('/Videos/'):self.send_error(404);return
  content=short_video if '/episode-' in self.path else video
  start=0;end=len(content)-1
  if self.headers.get('Range'):
   segment=self.headers['Range'].split('=')[1].split('-');start=int(segment[0] or 0);end=min(end,int(segment[1])) if segment[1] else end;self.send_response(206);self.send_header('Content-Range',f'bytes {start}-{end}/{len(content)}')
  else:self.send_response(200)
  self.send_header('Accept-Ranges','bytes');self.send_header('Content-Length',str(end-start+1));self.send_header('Content-Type','video/mp4' if content is short_video else 'video/x-matroska');self.end_headers()
  try:self.wfile.write(content[start:end+1])
  except (BrokenPipeError,ConnectionResetError):pass
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start()
folder=r/'profile/config/aster';folder.mkdir(parents=True,exist_ok=True);(folder/'settings.json').write_text(json.dumps({'fullscreen':False,'remember_session':False,'hardware_decode':False,'volume':0,'ui_scale':float(os.environ.get('ASTER_TEST_SCALE','1.0'))}))
sock=r/'control/embedded.sock';sock.unlink(missing_ok=True)
env=os.environ.copy();env.update(XDG_CONFIG_HOME=str(folder.parent),XDG_DATA_HOME=str(r/'profile/data'),ASTER_PREVIEW_DATA=str(r/'layout-fixture.json'),ASTER_PREVIEW_ROUTE='',ASTER_DEV_BRIDGE=str(sock),ASTER_DEV_FIXTURE_SERVER=f'http://127.0.0.1:{server.server_port}/',ASTER_WINDOW_TITLE='Aster - Performance audit',ASTER_WINDOW_SIZE='1920x1080')
if os.environ.get('ASTER_AUDIT_LOGIN')=='1':env.pop('ASTER_PREVIEW_DATA',None)
log=open(r/'app.log','w');app=subprocess.Popen([str(binary),'--windowed'],env=env,stdin=subprocess.DEVNULL,stdout=log,stderr=log,start_new_session=True)
def call(v):
 with socket.socket(socket.AF_UNIX) as s:s.settimeout(3);s.connect(str(sock));s.sendall((json.dumps(v)+'\n').encode());return json.loads(s.makefile().readline())
def wait(test,description):
 deadline=time.monotonic()+45;last={}
 while time.monotonic()<deadline:
  if app.poll() is not None:raise RuntimeError('App exited: '+(r/'app.log').read_text()[-3000:])
  try:
   last=call({'command':'status'})
   if test(last):return last
  except (OSError,ValueError):pass
  time.sleep(.05)
 raise AssertionError(description+': '+json.dumps(last))
def action(a):call({'command':'action','action':a});time.sleep(.1)
def command(*args):call({'command':'player-command','args':list(args)});time.sleep(.1)
def capture(name):
 path=r/name;path.unlink(missing_ok=True);call({'command':'capture','path':str(path)})
 deadline=time.monotonic()+10
 while time.monotonic()<deadline:
  if path.exists() and path.read_bytes().endswith(b'IEND\xaeB\x60\x82'):return
  time.sleep(.05)
 raise AssertionError('Screenshot did not finish: '+name)

# /proc counters are sampled without privileged profilers or system changes.
seconds=float(os.environ.get('ASTER_AUDIT_SECONDS','4'))
rows=[];hw=os.environ.get('ASTER_AUDIT_HWDEC')=='1'
def metrics():
 stat=pathlib.Path(f'/proc/{app.pid}/stat').read_text().rsplit(')',1)[1].split()
 gpu={};seen=set()
 for fd in pathlib.Path(f'/proc/{app.pid}/fdinfo').glob('*'):
  try:fields=dict(l.split(':',1) for l in fd.read_text().splitlines() if ':' in l)
  except OSError:continue
  client=fields.get('drm-client-id','').strip()
  if not client or client in seen:continue
  seen.add(client)
  for key,value in fields.items():
   if key.startswith('drm-engine-'):gpu[key]=gpu.get(key,0)+int(value.strip().split()[0])
 rss=int(next(l.split()[1] for l in pathlib.Path(f'/proc/{app.pid}/status').read_text().splitlines() if l.startswith('VmRSS:')))
 temp=None
 for folder in pathlib.Path('/sys/class/hwmon').glob('hwmon*'):
  try:
   if (folder/'name').read_text().strip()=='coretemp':temp=int((folder/'temp1_input').read_text())/1000
  except OSError:pass
 threads={}
 for folder in pathlib.Path(f'/proc/{app.pid}/task').glob('*'):
  try:
   parts=(folder/'stat').read_text().rsplit(')',1)[1].split();threads[folder.name]=((folder/'comm').read_text().strip(),int(parts[11])+int(parts[12]))
  except OSError:pass
 return {'threads':threads,'cpu':int(stat[11])+int(stat[12]),'rss_kib':rss,'gpu':gpu,'temp_c':temp,'t':time.monotonic()}
def mpv_probe():
 try:
  d=call({'command':'status'});path=d.get('performance',{}).get('mpv_socket')
  if not path:
   paths=sorted(pathlib.Path('/tmp').glob('aster-*/mpv.sock'),key=lambda p:p.stat().st_mtime,reverse=True);path=str(paths[0])
  output={}
  for index,name in enumerate(['pause','time-pos','hwdec-current','video-codec','video-params','container-fps','vo-delayed-frame-count','decoder-frame-drop-count']):
   with socket.socket(socket.AF_UNIX) as stream:
    stream.settimeout(2);stream.connect(path);stream.sendall((json.dumps({'command':['get_property',name],'request_id':index})+'\n').encode());reply=json.loads(stream.makefile().readline());output[name]=reply.get('data')
  return output
 except (OSError,ValueError,IndexError):return {}
def measure(name,drive=None):
 time.sleep(1)
 d0=call({'command':'status'});a=metrics();latencies=[]
 while time.monotonic()-a['t']<seconds:
  if drive:
   start=time.monotonic();drive();latencies.append((time.monotonic()-start)*1000);time.sleep(.08)
  else:time.sleep(.2)
 b=metrics();d1=call({'command':'status'});elapsed=b['t']-a['t']
 rate=lambda field:(d1.get('performance',{}).get(field,0)-d0.get('performance',{}).get(field,0))/elapsed
 result={'mode':name,'seconds':round(elapsed,2),'cpu_percent_one_core':round((b['cpu']-a['cpu'])/os.sysconf('SC_CLK_TCK')/elapsed*100,2),'rss_mib':round(b['rss_kib']/1024,2),'temperature_c':b['temp_c'],'ui_fps':round(rate('ui_frames'),2) if 'performance' in d1 else None,'render_fps':round(rate('render_calls'),2) if 'performance' in d1 else None,'gpu_percent':{key:round(max(0,value-a['gpu'].get(key,value))/elapsed/1e9*100,2) for key,value in b['gpu'].items()},'playing':d1.get('player_active',False)}
 result['repaint_causes']=d1.get('repaint_causes',[])[:5]
 result['input_events']=d1.get('input_events',[])
 result['thread_cpu_percent']={name+':'+tid:round((ticks-a['threads'].get(tid,('',ticks))[1])/os.sysconf('SC_CLK_TCK')/elapsed*100,2) for tid,(name,ticks) in b['threads'].items() if ticks>a['threads'].get(tid,('',ticks))[1]}
 if latencies:
  result['command_roundtrip_ms_max']=round(max(latencies),2);result['command_roundtrip_ms_p95']=round(sorted(latencies)[int(.95*(len(latencies)-1))],2)
 if d1.get('player_active'):result['decoder']=mpv_probe()
 rows.append(result);(r/'performance.json').write_text(json.dumps(rows,indent=2));print(json.dumps(result),flush=True)
def start_video():
 call({'command':'play-fixture','ticks':0,'hardware':hw})
 wait(lambda d:d['player_active'] and d['position']>.2,'playback benchmark start')
def stop_video():
 action('back');wait(lambda d:not d['player_active'],'stop benchmark video')
try:
 wait(lambda d:d.get('ready') and d.get('embedded'),'startup')
 if os.environ.get('ASTER_AUDIT_LOGIN')=='1':
  measure('signin_idle');call({'command':'focus','index':0});action('confirm');measure('signin_keyboard_idle')
 elif os.environ.get('ASTER_AUDIT_ONLY_PLAYBACK')=='1':
  start_video();time.sleep(4);measure('playback_hidden_controls');action('confirm');wait(lambda d:d['paused'],'pause audit');time.sleep(3);measure('playback_paused');stop_video()
 else:
  measure('discover_idle')
  action('down')
  direction=[False]
  def browse():
   direction[0]=not direction[0];old=call({'command':'status'})['focus'];call({'command':'action','action':'right' if direction[0] else 'left'});deadline=time.monotonic()+.4
   while time.monotonic()<deadline:
    if call({'command':'status'})['focus']!=old:break
    time.sleep(.005)
  measure('discover_navigation',browse)
  for index,title in [(1,'movies'),(2,'series'),(5,'settings')]:
   call({'command':'tab','index':index});time.sleep(.4);measure(title+'_idle')
   if index in [1,2]:action('down');measure(title+'_navigation',browse)
  call({'command':'tab','index':0});action('search');measure('search_keyboard_idle');action('back')
  if os.environ.get('ASTER_AUDIT_MODERN')=='1':
   query=[False]
   def search_update():
    query[0]=not query[0];call({'command':'search-fixture','query':'Title 000' if query[0] else ''});target=10 if query[0] else 360;deadline=time.monotonic()+.4
    while time.monotonic()<deadline:
     if call({'command':'status'})['items']==target:break
     time.sleep(.005)
   measure('search_updates',search_update);call({'command':'search-fixture','query':''})
  # Each mode starts a fresh fixture to keep it within the 30-second clip.
  start_video();time.sleep(4);measure('playback_hidden_controls');stop_video()
  if os.environ.get('ASTER_AUDIT_MODERN')=='1':
   start_video();action('up');measure('timeline_preview_playing');stop_video()
  start_video();action('confirm');wait(lambda d:d['paused'],'pause audit clip');measure('playback_paused');action('up');measure('timeline_preview_paused');stop_video()
  # Menu workload uses native/local chapters, injected only into this fixture.
  start_video();action('confirm');wait(lambda d:d['paused'],'pause chapter menu audit');command('set_property','chapter-list',[{'time':0,'title':'Opening'},{'time':10,'title':'Main story'},{'time':20,'title':'Credits'}]);command('script-message','aster-menu','chapters');measure('chapter_picker_idle');command('script-message','aster-menu','audio');measure('audio_menu_idle');command('script-message','aster-menu','sub');measure('subtitle_menu_idle');stop_video()
  start_video();call({'command':'fullscreen','enabled':True});time.sleep(4);measure('fullscreen_playback');stop_video();call({'command':'fullscreen','enabled':False})
finally:
 try:call({'command':'close'});app.wait(timeout=8)
 except (OSError,subprocess.TimeoutExpired):app.terminate();app.wait(timeout=5)
 server.shutdown();log.close()
