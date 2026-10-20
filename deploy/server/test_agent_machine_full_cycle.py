"""Executable contract-double end-to-end: inbox -> Herdr subprocess -> observation -> Git handoff.

This deliberately does not claim that Orbit's Rust HTTP server was exercised.
"""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from threading import Thread
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('machine_worker_cycle', Path(__file__).with_name('agent-machine-worker.py'))
w = importlib.util.module_from_spec(spec)
spec.loader.exec_module(w)

class Authority(BaseHTTPRequestHandler):
    def log_message(self,*args): pass
    def send_json(self,code,data):
        raw=json.dumps(data).encode()
        self.send_response(code)
        self.send_header('Content-Type','application/json')
        self.send_header('Content-Length',str(len(raw)))
        self.end_headers();self.wfile.write(raw)
    def handle_request(self):
        s=self.server.state
        if self.headers.get('Authorization')!='Bearer secret' or self.headers.get('x-orbit-device-id')!='executor-A':
            return self.send_json(403,{'error':'unauthorized'})
        route=self.path.split('?')[0].removeprefix(w.BASE)
        data=json.loads(self.rfile.read(int(self.headers.get('Content-Length','0')))) if self.command=='POST' else {}
        s['calls'].append((route,data))
        job=s['job']
        if route=='inbox': return self.send_json(200,[] if s['claimed'] else [dict(job)])
        if route=='brief': return self.send_json(200,{'job':dict(job),'brief':{'title':'Fix the feature','acceptance_criteria':['tests pass']},'worktree':False})
        if route=='claim':
            if s['claimed']: return self.send_json(200,{'claimed':False})
            s['claimed']=True;s['token']=data['claim_token'];job['revision']+=1
            return self.send_json(200,{'claimed':True})
        if route in ('launch-result','observe','handoff'):
            if not s['claimed'] or data.get('claim_token')!=s['token'] or data.get('job_id')!=job['id']:
                return self.send_json(403,{'error':'wrong claim'})
            if route=='launch-result':
                s['sessions'].append(data);job['revision']+=1
                return self.send_json(200,dict(job))
            if route=='observe':
                s['observations'].append(data);job['revision']+=1
                return self.send_json(200,{'state':'waiting_review' if data['activity']=='idle' else 'running'})
            if route=='handoff':
                receipt=data['handoff_id']
                if receipt in s['handoffs'] and s['handoffs'][receipt]!=data: return self.send_json(409,{'error':'conflicting handoff'})
                s['handoffs'][receipt]=data
                return self.send_json(200,{'accepted':True})
        return self.send_json(404,{'error':'unknown route'})
    do_GET=handle_request
    do_POST=handle_request

class CompleteWorkerCycle(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        self.root=Path(self.temp.name);self.repo=self.root/'repo';self.repo.mkdir()
        subprocess.run(['git','init','-q',str(self.repo)],check=True)
        subprocess.run(['git','-C',str(self.repo),'config','user.email','test@example.test'],check=True)
        subprocess.run(['git','-C',str(self.repo),'config','user.name','Test'],check=True)
        (self.repo/'README.md').write_text('test\n')
        subprocess.run(['git','-C',str(self.repo),'add','README.md'],check=True)
        subprocess.run(['git','-C',str(self.repo),'commit','-qm','initial'],check=True)
        self.journals=self.root/'journals'
        self.state={'claimed':False,'token':None,'calls':[],'sessions':[],'observations':[],'handoffs':{},
                    'job':{'id':'job-1','runtime_kind':'herdr','agent_kind':'codex','machine_ref':'executor-A',
                           'repository_ref':str(self.repo),'revision':1,'updated_at_ms':1}}
        self.http=ThreadingHTTPServer(('127.0.0.1',0),Authority);self.http.state=self.state
        self.thread=Thread(target=self.http.serve_forever,daemon=True);self.thread.start()
        self.addCleanup(lambda:(self.http.shutdown(),self.thread.join(timeout=3),self.http.server_close()))
        self.server=f'http://127.0.0.1:{self.http.server_port}'
        self.herdr_log=self.root/'herdr-log.jsonl'
        fake=self.root/'herdr';fake.write_text('''#!/usr/bin/env python3
import json,os,sys
args=sys.argv[1:]
with open(os.environ['FAKE_HERDR_LOG'],'a') as f: f.write(json.dumps(args)+'\\n')
if args[:3]==['--json','workspace','create'] or args[:3]==['--json','worktree','create']:
 print(json.dumps({'result':{'root_pane':{'pane_id':'pane-1'}}}))
elif args[:3]==['--json','agent','get']:
 print(json.dumps({'result':{'agent':{'status':'idle'}}}))
else:print(json.dumps({'result':{}}))
''');fake.chmod(0o700)
        self.fake=str(fake)
        self.env=patch.dict(os.environ,{'FAKE_HERDR_LOG':str(self.herdr_log)})
        self.env.start();self.addCleanup(self.env.stop)
    def test_full_launch_idle_handoff_review_ready_and_no_relaunch(self):
        results=w.poll(self.server,'secret','executor-A',self.journals,executable=self.fake)
        self.assertEqual(results[0]['result'],'launched')
        self.assertEqual(len(self.state['sessions']),1)
        self.assertEqual(len(self.herdr_log.read_text().splitlines()),3)
        (self.repo/'src.txt').write_text('agent work in progress\n')
        status=w.reconcile(self.server,'secret','executor-A',self.journals,executable=self.fake)
        self.assertEqual(status[0]['result'],'waiting_review')
        self.assertEqual(self.state['observations'][-1]['activity'],'idle')
        self.assertEqual(len(self.state['handoffs']),1)
        evidence=next(iter(self.state['handoffs'].values()))['payload']
        self.assertIn('src.txt',evidence['changed_files'])
        self.assertEqual(evidence['validation']['status'],'not_reported')
        self.assertFalse(evidence['agent_authorship_verified'])
        self.assertEqual(w.poll(self.server,'secret','executor-A',self.journals),[])
        w.reconcile(self.server,'secret','executor-A',self.journals,executable=self.fake)
        self.assertEqual(len(self.state['handoffs']),1)
        self.assertEqual(len([json.loads(x) for x in self.herdr_log.read_text().splitlines() if 'create' in x]),1)
    def test_handoff_lost_ack_retries_identical_payload(self):
        w.poll(self.server,'secret','executor-A',self.journals,executable=self.fake)
        actual=w.api
        drops=[True]
        def flaky(server,token,device,route,payload=None):
            if route=='handoff' and drops[0]:
                drops[0]=False
                actual(server,token,device,route,payload)
                raise TimeoutError('ack lost')
            return actual(server,token,device,route,payload)
        with patch.object(w,'api',side_effect=flaky):
            first=w.reconcile(self.server,'secret','executor-A',self.journals,executable=self.fake)
        self.assertEqual(first[0]['result'],'evidence_pending_review')
        journal=json.loads(w.journal_path(self.journals,'job-1').read_text())
        first_payload=journal['evidence_pending']
        self.assertEqual(len(self.state['handoffs']),1)
        again=w.reconcile(self.server,'secret','executor-A',self.journals,executable=self.fake)
        self.assertEqual(again[0]['result'],'waiting_review')
        self.assertEqual(len(self.state['handoffs']),1)
        requests=[payload for route,payload in self.state['calls'] if route=='handoff']
        self.assertEqual(requests,[first_payload,first_payload])
        self.assertEqual(json.loads(w.journal_path(self.journals,'job-1').read_text())['evidence_receipt'],first_payload['handoff_id'])
    def test_new_commit_is_attributed_to_job_period_not_prior_history(self):
        self.assertEqual(w.poll(self.server,'secret','executor-A',self.journals,executable=self.fake)[0]['result'],'launched')
        baseline=json.loads(w.journal_path(self.journals,'job-1').read_text())['baseline_head']
        (self.repo/'implemented.txt').write_text('new feature\n')
        subprocess.run(['git','-C',str(self.repo),'add','implemented.txt'],check=True)
        subprocess.run(['git','-C',str(self.repo),'commit','-qm','feature'],check=True)
        w.reconcile(self.server,'secret','executor-A',self.journals,executable=self.fake)
        evidence=next(iter(self.state['handoffs'].values()))['payload']
        self.assertEqual(evidence['git']['launch_baseline'],baseline)
        self.assertEqual(len(evidence['commits']),1)
        self.assertIn('implemented.txt',evidence['changed_files'])
        self.assertNotIn(baseline,evidence['commits'])

    def test_history_rewrite_refuses_to_supply_unreliable_evidence(self):
        w.poll(self.server,'secret','executor-A',self.journals,executable=self.fake)
        subprocess.run(['git','-C',str(self.repo),'checkout','--orphan','replacement'],capture_output=True,check=True)
        subprocess.run(['git','-C',str(self.repo),'commit','--allow-empty','-qm','replaced'],check=True)
        with self.assertRaisesRegex(ValueError,'baseline is not an ancestor'):
            entry=json.loads(w.journal_path(self.journals,'job-1').read_text())
            w.build_evidence(self.state['job'],{'worktree':False},entry['claim_token'],entry['baseline_head'])

    def test_other_machine_cannot_view_inbox(self):
        from urllib.error import HTTPError
        with self.assertRaises(HTTPError) as e:
            w.api(self.server,'secret','executor-B','inbox')
        self.assertEqual(e.exception.code,403)

if __name__=='__main__':unittest.main()
