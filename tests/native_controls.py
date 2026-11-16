import subprocess,socket,json,time,os,tempfile,sys
from pathlib import Path
root=Path(__file__).resolve().parents[1]
workspace=tempfile.TemporaryDirectory(prefix='aster-native-controls-')
folder=Path(sys.argv[1]).resolve() if len(sys.argv)>1 else Path(workspace.name)
folder.mkdir(parents=True,exist_ok=True)
(folder/'english.srt').write_text('1\n00:00:00,000 --> 00:00:30,000\nAster subtitle test · English\n')
(folder/'czech.srt').write_text('1\n00:00:00,000 --> 00:00:30,000\nTest titulků Aster · Čeština\n')
subprocess.run(['ffmpeg','-hide_banner','-loglevel','error','-f','lavfi','-i','color=c=0x18222f:s=1280x720:r=24','-f','lavfi','-i','sine=frequency=440:sample_rate=48000','-f','lavfi','-i','sine=frequency=880:sample_rate=48000','-i',str(folder/'english.srt'),'-i',str(folder/'czech.srt'),'-map','0:v','-map','1:a','-map','2:a','-map','3:s','-map','4:s','-t','30','-c:v','libx264','-preset','ultrafast','-pix_fmt','yuv420p','-c:a','aac','-ac','2','-c:s','srt','-metadata:s:a:0','language=eng','-metadata:s:a:0','title=English test tone','-metadata:s:a:1','language=ces','-metadata:s:a:1','title=Český testovací tón','-metadata:s:s:0','language=eng','-metadata:s:s:0','title=English subtitles','-metadata:s:s:1','language=ces','-metadata:s:s:1','title=České titulky','-disposition:s:0','0','-disposition:s:1','0','-y',str(folder/'tracks.mkv')],check=True)
sock=folder/'native.sock'
sock.unlink(missing_ok=True)
log=(folder/'mpv.log').open('w')
p=subprocess.Popen(['mpv','--no-config','--osc=no','--force-window=yes','--fullscreen=no','--geometry=1280x720','--idle=yes','--keep-open=yes','--volume=0',f'--script={root}/assets/player/aster.lua',f'--input-ipc-server={sock}','--force-media-title=Aster playback test',str(folder/'tracks.mkv')],stdout=log,stderr=log)
try:
 deadline=time.monotonic()+8
 while not sock.exists():
  if p.poll() is not None:raise RuntimeError('mpv exited before IPC setup')
  if time.monotonic()>deadline:raise TimeoutError('No mpv IPC socket')
  time.sleep(.03)
 client=socket.socket(socket.AF_UNIX);client.settimeout(4);client.connect(str(sock));reader=client.makefile();serial=0
 def command(*args):
  global serial
  serial+=1;client.sendall((json.dumps({'command':list(args),'request_id':serial})+'\n').encode())
  while True:
   message=json.loads(reader.readline())
   if message.get('request_id')==serial:
    assert message.get('error')=='success',(args,message)
    return message.get('data')
 def get(prop):return command('get_property',prop)
 def wait(predicate,description):
  limit=time.monotonic()+5
  while time.monotonic()<limit:
   try:
    if predicate():return
   except (AssertionError,KeyError):pass
   time.sleep(.04)
  raise AssertionError(description)
 def event(name):command('script-message','aster-input',name);time.sleep(.08)
 wait(lambda:len(get('track-list'))>=5,'Multilingual tracks were not loaded')
 command('set_property','pause',True);command('set_property','sid','no')
 wait(lambda:get('user-data/aster')['visible'],'Overlay was not created')
 time.sleep(.35)
 command('screenshot-to-file',str(folder/'controls.png'),'window');time.sleep(.3)
 event('audio');wait(lambda:get('user-data/aster')['menu']=='audio','Audio menu did not open')
 command('screenshot-to-file',str(folder/'audio-menu.png'),'window');time.sleep(.3)
 event('down');event('confirm');wait(lambda:get('aid')==2,'Controller could not select the Czech audio track')
 event('subtitles');wait(lambda:get('user-data/aster')['cursor']==1,'Subtitles Off was not focused')
 event('down');event('down');event('confirm');wait(lambda:get('sid')==2,'Controller could not select Czech subtitles')
 event('subtitles');command('screenshot-to-file',str(folder/'subtitle-menu.png'),'window');time.sleep(.3)
 event('up');event('up');event('confirm');wait(lambda:get('sid') is False,'Subtitles Off did not disable subtitles')
 command('script-message','aster-menu','speed');time.sleep(.1);event('down');event('confirm');wait(lambda:abs(get('speed')-1.25)<.01,'Playback speed selection failed')
 event('menu');wait(lambda:get('user-data/aster')['controls'],'Controller control-bar focus did not open')
 event('menu');wait(lambda:not get('user-data/aster')['controls'],'Menu did not close the control bar')
 command('script-message','aster-up-next','Fixture series · S02E01 · The next chapter','7')
 wait(lambda:bool(get('user-data/aster')['up_next']),'Up next card did not open')
 command('screenshot-to-file',str(folder/'up-next.png'),'window');time.sleep(.3)
 event('confirm');wait(lambda:get('user-data/aster-autoplay-action')=='play','Play now did not request autoplay')
 command('script-message','aster-up-next','Fixture series · S02E01 · The next chapter','7')
 event('right');event('confirm');p.wait(timeout=5)
 print('PASS: Up next rendered; Play now and Cancel work; rendered controls; two named audio tracks; two subtitle tracks; controller selection; subtitles Off; playback speed; Menu toggles controls.')
finally:
 try:p.wait(timeout=5)
 except subprocess.TimeoutExpired:p.terminate();p.wait(timeout=5)
 log.close()
