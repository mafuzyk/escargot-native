"""Exercise native request acceptance and removal using an isolated Sway fixture.

Requires Pillow, Tesseract, Sway, grim, wtype and bubblewrap. The fixture root
must have runtime/, tmp/ and config/escargot-native/ directories. Account data
and screenshots stay under that fixture root; no real account is authenticated.
"""
from pathlib import Path
from PIL import Image, ImageOps
import os, subprocess, json, time, io, sys

assert len(sys.argv) == 3, 'Usage: ui_contact_flow.py ISOLATED_SWAY_FIXTURE CLIENT_BINARY'
p=Path(sys.argv[1]).resolve()
binary=Path(sys.argv[2]).resolve()
assert (p/'runtime').is_dir(), 'Use a separate headless Sway runtime for this fixture'
env=dict(os.environ,XDG_RUNTIME_DIR=str(p/'runtime'),WAYLAND_DISPLAY='wayland-1',TMPDIR=str(p/'tmp'))
socket=str(next((p/'runtime').glob('sway-ipc*.sock')))
def walk(node):
 yield node
 for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from walk(child)
def windows():
 tree=json.loads(subprocess.check_output(['swaymsg','-s',socket,'-t','get_tree','-r'],text=True))
 return [n for n in walk(tree) if n.get('app_id','') and n['app_id'].startswith('escargot-native-')]
def capture(name,app='add-contact'):
 for _ in range(50):
  found=[n for n in windows() if n['app_id']=='escargot-native-'+app]
  if found:break
  time.sleep(.1)
 else:raise AssertionError('Expected native window missing: '+app)
 time.sleep(.3)
 for _ in range(50):
  found=[n for n in windows() if n['app_id']=='escargot-native-'+app]
  assert found, 'The native window closed before capture'
  subprocess.run(['grim',str(p/'screen.png')],env=env,check=True)
  r=found[0]['rect'];im=Image.open(p/'screen.png').crop((r['x'],r['y'],r['x']+r['width'],r['y']+r['height'])).convert('RGB')
  if sum(im.getpixel((im.width//2,5)))>0:break
  time.sleep(.1)
 else:raise AssertionError('Window content did not match the compositor bounds')
 path=p/(name+'.png');im.save(path);return path

def text(path):
 image=Image.open(path).convert('RGB')
 if sum(image.resize((1,1)).getpixel((0,0)))<384:image=ImageOps.invert(image)
 image=image.convert('L').resize((image.width*3,image.height*3));b=io.BytesIO();image.save(b,format='PNG')
 return subprocess.check_output(['tesseract','stdin','stdout','--psm','11'],input=b.getvalue(),stderr=subprocess.DEVNULL).decode().casefold()
def keys(*args):
 subprocess.run(['bwrap','--bind','/','/','--bind',str(p/'tmp'),'/tmp','--die-with-parent','--','wtype','-s','200','-k','F12','-s','100','-d','25',*args],env=env,check=True);time.sleep(.3)


def ctrl(key):
 keys('-M','ctrl','-s','100','-k',key,'-s','100','-m','ctrl')
def remove_prompt():
 keys('-M','shift','-s','100','-k','Tab','-s','80','-k','Tab','-s','80','-k','Tab','-s','100','-m','shift','-s','100','-k','Return')
 shot=capture('remove-confirmation','contacts')
 subprocess.run(['python3','tests/ui_contact_requests.py',str(shot),'removal'],check=True)
 return shot

def run_case(night, mode):
 (p/'config/escargot-native/config.toml').write_text('night = '+str(night).lower()+'\nsingle_window = true\n')
 app_env=dict(env,XDG_CONFIG_HOME=str(p/'config'),XDG_DATA_HOME=str(p/'data'),ESCARGOT_DEMO_REQUESTS='1',ESCARGOT_DEMO_CHAT='alex@escargot.chat')
 app_env.pop('DISPLAY',None)
 with (p/('ui-'+mode+'.log')).open('w') as log:
  process=subprocess.Popen([str(binary),'--demo-settings'],env=app_env,stdout=log,stderr=log)
  try:
   for _ in range(300):
    assert process.poll() is None, 'The native fixture exited before its first frame'
    if any(n['app_id']=='escargot-native-contacts' for n in windows()):break
    time.sleep(.1)
   else:raise AssertionError('The native fixture did not render its first frame')
   shot=capture('requests-'+mode,'contacts')
   subprocess.run(['python3','tests/ui_contact_requests.py',str(shot)],check=True)
   ctrl('f')
   keys('-k','Tab')
   if mode=='defer':
    keys('-k','Tab','-s','100','-k','Tab','-s','100','-k','Return')
    assert 'contact requests' not in text(capture('deferred-request','contacts')), 'Not now must dismiss the request'
    keys('taylor')
    assert 'no matching contacts' in text(capture('deferred-not-added','contacts')), 'Deferring must not add the sender'
    print('PASS: pending-request deferral in Night',flush=True)
    return
   keys('-k','Tab','-s','100','-k','Return')
   accepted=text(capture('accepted-request','contacts'))
   assert 'contact requests' not in accepted, 'Accepting a request must clear it without opening another dialog'
   ctrl('f');keys('taylor')
   added=text(capture('request-added-list','contacts'))
   assert 'taylor' in added, 'Accepted requests must appear in the contact list'
   assert 'pending' in added, 'Accepting must show the outgoing pending state until the other side confirms'
   print('PASS: native request acceptance and contact update',flush=True)
   ctrl('f');ctrl('a');keys('-k','BackSpace')
   ctrl('Tab');ctrl('Tab')
   remove_prompt()
   keys('-k','Escape')
   assert 'from your contacts?' not in text(capture('remove-escape','contacts')), 'Escape must cancel removal'
   remove_prompt()
   keys('-k','Tab','-s','100','-k','Return')
   cancelled=text(capture('remove-cancelled','contacts'))
   sidebar=Image.open(p/'remove-cancelled.png')
   sidebar.crop((0,150,300,sidebar.height-85)).save(p/'cancelled-sidebar.png')
   assert 'from your contacts?' not in cancelled and 'alex' in text(p/'cancelled-sidebar.png'), 'Cancel must keep the contact available in the list'
   print('PASS: removal cancellation with Escape and Cancel',flush=True)
   remove_prompt()
   keys('-k','Tab','-k','Tab','-s','100','-k','Return')
   removed=text(capture('removed-contact','contacts'))
   assert 'contact removed' in removed and 'no wine' in removed, 'Removing a contact must preserve the conversation history'
   ctrl('f');keys('alex')
   assert 'no matching contacts' in text(capture('removed-list','contacts')), 'Confirmed removal must remove the contact from local search'
   print('PASS: confirmed removal preserves messages and updates contacts',flush=True)
  finally:
   process.terminate();process.wait(timeout=10)
   time.sleep(.3)
if __name__ == '__main__':
 run_case(False,'accept-remove')
 run_case(True,'defer')
