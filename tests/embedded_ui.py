"""Embedded GL smoke test: Python 3, ffmpeg, libmpv and a graphical desktop.
Usage: python tests/embedded_ui.py /absolute/path/to/aster [artifact-directory]
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
(r/'chapters.ffmeta').write_text(';FFMETADATA1\n'+''.join(f'[CHAPTER]\nTIMEBASE=1/1000\nSTART={start}\nEND={end}\ntitle={title}\n' for start,end,title in [(0,10000,'Opening'),(10000,20000,'Main story'),(20000,30000,'Credits')]))

(r/'english.srt').write_text('1\n00:00:00,000 --> 00:00:30,000\nAster embedded subtitle test · English\n')
(r/'czech.srt').write_text('1\n00:00:00,000 --> 00:00:30,000\nTest titulků Aster · Čeština\n')
subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-f','lavfi','-i','testsrc2=size=960x540:rate=24','-f','lavfi','-i','sine=frequency=440:sample_rate=48000','-f','lavfi','-i','sine=frequency=880:sample_rate=48000','-i',str(r/'english.srt'),'-i',str(r/'czech.srt'),'-f','ffmetadata','-i',str(r/'chapters.ffmeta'),'-map_metadata','5','-map_chapters','-1' if server_chapters else '5','-map','0:v','-map','1:a','-map','2:a','-map','3:s','-map','4:s','-t','30','-c:v','libx264','-preset','ultrafast','-c:a','aac','-c:s','srt','-metadata:s:a:0','language=eng','-metadata:s:a:1','language=ces','-metadata:s:s:0','language=eng','-metadata:s:s:1','language=ces','-y',str(r/'clip.mkv')],check=True)
subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-i',str(r/'clip.mkv'),'-frames:v','1','-y',str(r/'atlas.png')],check=True)
(r/'layout-fixture.json').write_text(json.dumps({'items':[{'Id':'preview-0','Name':'Embedded playback sample','Type':'Movie','Overview':'A local visual fixture. No real media or server credentials are used.','RunTimeTicks':300000000}], 'atlas':str(r/'atlas.png'),'posters':[[0,0,960,540]],'backdrop':[0,0,960,540]}))
trickplay_enabled=os.environ.get('ASTER_TEST_TRICKPLAY')=='1'
trickplay_sheet=(r/'atlas.png').read_bytes();preview_requests=[]
video=(r/'clip.mkv').read_bytes();short_video=(root/'tests/fixtures/playback.mp4').read_bytes();reports=[]
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
env=os.environ.copy();env.update(XDG_CONFIG_HOME=str(folder.parent),XDG_DATA_HOME=str(r/'profile/data'),ASTER_PREVIEW_DATA=str(r/'layout-fixture.json'),ASTER_PREVIEW_ROUTE='detail',ASTER_DEV_BRIDGE=str(sock),ASTER_DEV_FIXTURE_SERVER=f'http://127.0.0.1:{server.server_port}/',ASTER_WINDOW_INACTIVE='1',ASTER_WINDOW_TITLE='Aster - Embedded playback verification',ASTER_WINDOW_SIZE='1280x800')
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
try:
 d=wait(lambda d:d.get('ready') and d.get('embedded') and d.get('hero'),'libmpv startup');print('Backend initialized',flush=True)
 assert d['hero'][0]==0 and abs(d['hero'][2]-d['viewport'][2])<1,'hero must reach both window edges'
 capture('edge-to-edge-preview.png')
 call({'command':'play-fixture','ticks':12_000_000});d=wait(lambda d:d['player_active'] and d['position']>1.25 and d['duration']>0,'embedded playback/resume');action('confirm');wait(lambda d:d['paused'],'pause')
 assert d['focused_card']==d['viewport'],'video must cover the existing client area'
 children=pathlib.Path(f'/proc/{app.pid}/task/{app.pid}/children').read_text().strip();assert os.environ.get('ASTER_TEST_FLATPAK')=='1' or not children,'no external player process is permitted'
 time.sleep(.5)
 idle_before=call({'command':'status'}).get('performance',{}).get('ui_frames',0);time.sleep(.8);idle_after=call({'command':'status'}).get('performance',{}).get('ui_frames',0)
 assert idle_after-idle_before<12,'paused player must not repaint continuously'
 capture('embedded-player.png');print('In-window frame, resume, paused repaint budget, and no child player verified',flush=True)
 wait(lambda d:d['player_ui'].get('chapter_count')==3 and d['player_ui'].get('chapter_title')==chapter_names[0],'native or server chapter titles must load')
 assert call({'command':'status'})['player_ui']['chapter_markers']==[10,20],'chapter boundaries must be positioned on the timeline'
 before=call({'command':'status'})['position']
 action('right');wait(lambda d:d['player_ui'].get('controls') and d['player_ui'].get('focus')==2,'Right must focus rewind without seeking')
 action('right');wait(lambda d:d['player_ui'].get('focus')==3,'Right must focus forward without seeking')
 assert abs(call({'command':'status'})['position']-before)<0.15,'focus navigation must not seek'
 action('left');wait(lambda d:d['player_ui'].get('focus')==2,'Left moves focus back')
 action('left');wait(lambda d:d['player_ui'].get('focus')==1,'Left returns focus to pause')
 volume=call({'command':'status'})['player_ui']['volume']
 action('up');wait(lambda d:d['player_ui'].get('timeline') is True,'Up focuses timeline')
 expected_preview='trickplay' if trickplay_enabled else 'decoded-frame'
 wait(lambda d:d.get('seek_preview_source')==expected_preview,'thumbnail preview must load without changing playback')
 capture('embedded-focused-timeline.png')
 action('right');wait(lambda d:d['position']>=before+9.5,'Right seeks forward on timeline')
 wait(lambda d:d.get('seek_preview_position') is not None and d['seek_preview_position']>=before+9.0,'thumbnail must follow the new seek position')
 wait(lambda d:d['player_ui'].get('chapter_title')==chapter_names[1],'chapter title must follow seek position')
 capture('embedded-thumbnail-forward.png')
 action('left');wait(lambda d:abs(d['position']-before)<0.3,'Left seeks backward on timeline')
 assert call({'command':'status'})['paused'],'timeline seeking must preserve pause state'
 action('right');wait(lambda d:d['position']>=before+9.5,'initial hold step remains ten seconds')
 held_since=time.monotonic();steps=set()
 while time.monotonic()-held_since<6 and not {30,60}.issubset(steps):
  action('right-repeat');steps.add(call({'command':'status'})['player_ui'].get('seek_step'))
 assert 30 in steps and 60 in steps,'held seeking must ramp through 30s and 60s steps: '+str(steps)
 command('seek',0,'absolute','exact');wait(lambda d:d['position']<0.2,'reset paused fixture position')
 action('right-repeat');wait(lambda d:d['position']>20,'accelerated step must seek further than a tap')
 action('seek-release');wait(lambda d:d['player_ui'].get('seek_step')==10,'release resets acceleration')
 end_position=call({'command':'status'})['position']
 action('left');wait(lambda d:abs(d['position']-(end_position-10))<0.3,'new tap after release seeks ten seconds')
 action('right');wait(lambda d:d['player_ui'].get('seek_step')==10 and d['position']>end_position-0.3,'direction change starts with a ten-second step')
 action('seek-release')
 # Click the 20-second boundary; chapter hit boxes must override general scrubbing.
 capture('embedded-chapter-markers.png');chapter_width,chapter_height=struct.unpack('>II',(r/'embedded-chapter-markers.png').read_bytes()[16:24])
 duration=call({'command':'status'})['duration'];left=64*chapter_height/720;right=chapter_width-70*chapter_height/720
 command('mouse',round(left+(right-left)*20/duration),round(657*chapter_height/720));command('keypress','MBTN_LEFT')
 wait(lambda d:abs(d['position']-20)<0.25,'clicking a chapter marker seeks to its exact start')
 assert call({'command':'status'})['paused'],'chapter selection must preserve pause state'
 # Return the mouse to the video so hover cannot override current chapter selection.
 command('mouse',round(chapter_width/2),round(chapter_height/2))
 command('seek',before,'absolute','exact');wait(lambda d:abs(d['position']-before)<0.3,'restore fixture position after acceleration')
 action('chapters');wait(lambda d:d['player_ui'].get('menu')=='chapters','Y opens chapter picker')
 capture('embedded-chapter-picker.png')
 action('down');wait(lambda d:d['player_ui'].get('cursor')==2,'D-pad selects next chapter')
 action('confirm');wait(lambda d:d['player_ui'].get('menu')=='none' and abs(d['position']-10)<0.25,'A accepts selected chapter')
 assert call({'command':'status'})['paused'],'chapter picker must preserve pause state'
 command('seek',before,'absolute','exact');wait(lambda d:abs(d['position']-before)<0.3,'restore fixture after chapter picker')


 action('down');wait(lambda d:d['player_ui'].get('timeline') is False and d['player_ui'].get('focus')==1,'Down restores previous player button')
 assert call({'command':'status'})['player_ui']['volume']==volume,'Up/Down changes focus rather than volume'
 action('right');wait(lambda d:d['player_ui'].get('focus')==2,'Right returns to button focus navigation')
 assert abs(call({'command':'status'})['position']-before)<0.3,'button navigation must not seek after leaving timeline'
 action('left');wait(lambda d:d['player_ui'].get('focus')==1,'restore pause focus')

 action('menu');wait(lambda d:not d['player_ui'].get('controls'),'dismiss focus before mouse tests')

 height=struct.unpack('>II',(r/'embedded-player.png').read_bytes()[16:24])[1]
 command('mouse',round(24*height/720),round(693*height/720));command('keypress','MBTN_LEFT');wait(lambda d:not d['paused'],'mouse play button');command('mouse',round(400*height/720),round(300*height/720));command('keypress','MBTN_LEFT');wait(lambda d:d['paused'],'video click pauses');command('keypress','MBTN_LEFT');wait(lambda d:not d['paused'],'video click resumes');action('confirm');wait(lambda d:d['paused'],'pause after mouse playback')
 action('audio');wait(lambda d:d['player_ui'].get('menu')=='audio','audio menu');capture('embedded-audio.png');action('down');action('confirm');wait(lambda d:d['player_ui'].get('audio')==2,'audio selection')
 action('subtitles');wait(lambda d:d['player_ui'].get('menu')=='sub','subtitle menu');action('down');action('down');action('confirm');wait(lambda d:d['player_ui'].get('subtitle')==2,'subtitle selection')
 action('subtitles');capture('embedded-subtitles.png');action('up');action('up');action('confirm');wait(lambda d:d['player_ui'].get('subtitle') is False,'subtitles Off')
 command('script-message','aster-menu','speed');action('down');action('confirm')
 action('menu');wait(lambda d:d['player_ui'].get('controls'),'controller controls');action('menu');wait(lambda d:not d['player_ui'].get('controls'),'controller controls dismissed')
 call({'command':'resize','width':960,'height':640});time.sleep(.4);d=call({'command':'status'});assert d['focused_card']==d['viewport'];capture('embedded-player-resized.png')
 command('set_property','fullscreen',True);wait(lambda d:d['fullscreen'],'fullscreen on');command('set_property','fullscreen',False);wait(lambda d:not d['fullscreen'],'fullscreen off')
 action('menu');wait(lambda d:d['player_ui'].get('controls'),'controls before Back');action('back');wait(lambda d:not d['player_active'],'Back immediately stops from controls')
 call({'command':'play-fixture','ticks':0});d=wait(lambda d:d['player_active'] and d['position']>0 and d['position']<2,'second playback on same core');command('script-message','aster-up-next','Fixture series · S02E01 · Next episode','7');wait(lambda d:bool(d['player_ui'].get('up_next')),'embedded Up next');capture('embedded-up-next.png');action('back');wait(lambda d:not d['player_active'],'second stop')
 call({'command':'play-fixture','ticks':0,'episodes':True})
 wait(lambda d:d['player_active'] and 'S01E01' in d['playing'] and d['position']>0,'manual navigation fixture');wait(lambda d:d['player_ui'].get('chapter_count')==0,'chapters must not leak into the next file');action('chapters');assert call({'command':'status'})['player_ui'].get('menu')=='none','Y must not open an empty chapter picker';action('confirm');wait(lambda d:d['paused'],'pause manual fixture')
 action('previous');wait(lambda d:d['player_ui'].get('previous') is False,'first episode boundary must not stop or wrap')
 action('next');wait(lambda d:'Episode 2' in d['playing'] and 0<d['position']<1,'RB next episode starts from zero');action('confirm');wait(lambda d:d['paused'],'pause next episode')
 action('next');wait(lambda d:d['player_ui'].get('next') is False,'last episode boundary must not stop or wrap')
 capture('embedded-episode-controls.png');episode_width,episode_height=struct.unpack('>II',(r/'embedded-episode-controls.png').read_bytes()[16:24])
 command('mouse',round(episode_width-246*episode_height/720),round(693*episode_height/720));command('keypress','MBTN_LEFT')
 wait(lambda d:'Episode 1' in d['playing'] and 0<d['position']<1,'mouse previous episode starts from zero');action('back');wait(lambda d:not d['player_active'],'stop after manual navigation')
 call({'command':'play-fixture','ticks':0,'episodes':True})
 wait(lambda d:d['player_active'] and bool(d['up_next']) and bool((d['player_ui'] or {}).get('up_next')),'natural EOF Up next must remain visible')
 capture('embedded-natural-up-next.png');action('confirm')
 wait(lambda d:d['player_active'] and 'Episode 2' in d['playing'] and 0<d['position']<1.0,'Play now must load the next episode from zero in the same window')
 action('audio');wait(lambda d:d['player_ui'].get('menu')=='audio','menu before Back');action('back');wait(lambda d:not d['player_active'],'Back immediately stops from audio menu')
 call({'command':'play-fixture','ticks':0});wait(lambda d:d['player_active'] and d['position']>0,'playback before app shutdown')
 call({'command':'close'});app.wait(timeout=8)
 assert app.returncode==0,'active-playback shutdown must free the GL renderer cleanly'
 assert preview_requests,'seek previews must request authenticated metadata'
 if trickplay_enabled:assert any('/Trickplay/' in p for p in preview_requests),'server trickplay tile must be fetched'
 print('PASS: authenticated '+expected_preview+' thumbnail previews and chapter markers and controller picker, embedded GL rendering, mouse/controller focus and accelerated timeline seeking, audio/subtitles/Off, resize, fullscreen, Stop, repeat playback, manual previous/next/boundaries, natural EOF/Up next/Play now, active-playback shutdown and full-bleed hero.',flush=True)
 assert len([p for p,body in reports if p=='/Sessions/Playing'])==8,'all playback sessions must actually load'
 (r/'reports.json').write_text(json.dumps(reports,indent=2))
finally:
 try:call({'command':'close'});app.wait(timeout=8)
 except (OSError,subprocess.TimeoutExpired):app.terminate();app.wait(timeout=5)
 server.shutdown();log.close()
 assert app.returncode==0,'app must exit cleanly after freeing its GL renderer'
