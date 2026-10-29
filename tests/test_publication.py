import json,os,pathlib,subprocess,tempfile,unittest
ROOT=pathlib.Path(__file__).resolve().parents[1]
FAKE='''#!/usr/bin/env python3
import json,os,sys
p=os.environ['FAKE_STATE'];s=json.load(open(p));a=sys.argv[1:];s['calls'].append(a)
if a[0]=='api':
 if '--slurp' in a: print(json.dumps([s['releases']]))
 elif a[1].endswith('/latest'): print(json.dumps(next(r for r in s['releases'] if r['tag_name']==s['latest'])))
 else:
  tag=a[1].split('/')[-1];r=next((r for r in s['releases'] if r['tag_name']==tag),None)
  if r: print(json.dumps(r))
  else: json.dump(s,open(p,'w'));sys.exit(1)
elif a[:2]==['release','create']:
 s['releases'].append({'tag_name':a[2],'target_commitish':a[a.index('--target')+1],'draft':True,'prerelease':False})
elif a[:2]==['release','edit']:
 r=next(r for r in s['releases'] if r['tag_name']==a[2])
 if '--draft=false' in a:r['draft']=False
 if '--latest' in a:s['latest']=a[2]
json.dump(s,open(p,'w'))
'''
class PublicationTests(unittest.TestCase):
 def run_release(self,releases,number):
  with tempfile.TemporaryDirectory() as d:
   p=pathlib.Path(d);(p/'dist').mkdir();(p/'dist/test.AppImage').write_bytes(b'fixture');(p/'gh').write_text(FAKE);(p/'gh').chmod(0o755)
   state={'releases':releases,'latest':releases[0]['tag_name'] if releases else None,'calls':[]};(p/'state.json').write_text(json.dumps(state))
   env={**os.environ,'PATH':d+os.pathsep+os.environ['PATH'],'FAKE_STATE':str(p/'state.json'),'GITHUB_REPOSITORY':'owner/repo','GITHUB_SHA':'abc123','GITHUB_RUN_NUMBER':str(number)}
   result=subprocess.run(['python3',str(ROOT/'scripts/release.py')],cwd=p,env=env,capture_output=True,text=True)
   return result,json.loads((p/'state.json').read_text())
 def test_first_release_is_published(self):
  r,s=self.run_release([],1);self.assertEqual(r.returncode,0,r.stderr);self.assertEqual(s['latest'],'v0.1.1');self.assertFalse(s['releases'][0]['draft'])
 def test_old_build_does_not_replace_newer_latest(self):
  release={'tag_name':'v0.1.43','target_commitish':'newer','draft':False,'prerelease':False}
  r,s=self.run_release([release],42);self.assertEqual(r.returncode,0,r.stderr);self.assertEqual(s['latest'],'v0.1.43')
 def test_published_release_is_not_overwritten(self):
  release={'tag_name':'v0.1.42','target_commitish':'abc123','draft':False,'prerelease':False}
  r,s=self.run_release([release],42);self.assertEqual(r.returncode,0,r.stderr);self.assertFalse(any(a[:2]==['release','upload'] for a in s['calls']))
