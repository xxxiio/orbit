"""Executable local HTTP boundary tests (no Rust binary / Herdr required).

This is a protocol double, not proof of the Orbit Rust server's behavior.
"""
import importlib.util
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import tempfile
from threading import Thread
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('machine_worker', Path(__file__).with_name('agent-machine-worker.py'))
w = importlib.util.module_from_spec(spec)
spec.loader.exec_module(w)

class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args): pass
    def respond(self, code, data):
        content=json.dumps(data).encode()
        self.send_response(code)
        self.send_header('Content-Type','application/json')
        self.send_header('Content-Length',str(len(content)))
        self.end_headers()
        self.wfile.write(content)
    def handle_request(self):
        state=self.server.contract_state
        if self.headers.get('Authorization')!='Bearer test-token' or self.headers.get('x-orbit-device-id')!='machine':
            self.respond(403,{'error':'forbidden'});return
        key=self.path.split('?')[0].removeprefix(w.BASE)
        if self.command=='POST':
            size=int(self.headers['Content-Length'])
            body=json.loads(self.rfile.read(size))
        else: body={}
        state['requests'].append((key,body))
        job=state['job']
        if key=='inbox': self.respond(200, [] if state['claimed'] else [job]);return
        if key=='brief': self.respond(200,{'job':job,'brief':{'title':'Task'},'worktree':False});return
        if key=='claim':
            if state['claimed']:
                self.respond(200,{'claimed':False,'job':job});return
            state['claimed']=True;job['revision']+=1
            self.respond(200,{'claimed':True,'job':job});return
        if key=='launch-result':
            state['launch_results'].append(body)
            job['revision']+=1
            self.respond(200,job);return
        if key=='observe':
            state['observations'].append(body)
            job['revision']+=1
            self.respond(200,{'state':'running','revision':job['revision']});return
        self.respond(404,{'error':'no route'})
    do_GET=handle_request
    do_POST=handle_request

class HttpWorkerContracts(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup)
        self.folder=Path(self.tmp.name)
        repo=self.folder/'repo';repo.mkdir()
        state={'requests':[],'claimed':False,'launch_results':[],'observations':[],
               'job':{'id':'job','runtime_kind':'herdr','agent_kind':'codex','machine_ref':'machine',
                      'repository_ref':str(repo),'revision':1,'updated_at_ms':1}}
        self.http=ThreadingHTTPServer(('127.0.0.1',0),Handler)
        self.http.contract_state=state
        self.addCleanup(self.http.server_close)
        self.thread=Thread(target=self.http.serve_forever,daemon=True);self.thread.start()
        self.addCleanup(lambda:(self.http.shutdown(),self.thread.join(timeout=3)))
        self.state=state;self.server=f'http://127.0.0.1:{self.http.server_port}'
    def test_http_claim_launch_monitor_without_duplicate(self):
        with patch.object(w,'launch_runtime',return_value='agent-session') as runner:
            self.assertEqual(w.poll(self.server,'test-token','machine',self.folder)[0]['result'],'launched')
            self.assertEqual(w.poll(self.server,'test-token','machine',self.folder),[])
            with patch.object(w,'herdr',return_value={'result':{'agent':{'status':'working'}}}):
                self.assertEqual(w.reconcile(self.server,'test-token','machine',self.folder)[0]['result'],'running')
            self.assertEqual(runner.call_count,1)
        self.assertEqual(len(self.state['launch_results']),1)
        self.assertEqual(len(self.state['observations']),1)
        self.assertEqual(self.state['observations'][0]['session_ref'],'agent-session')
    def test_lost_launch_report_replayed_and_no_second_launch(self):
        base=w.api
        def outage(server,token,device,endpoint,payload=None):
            if endpoint=='launch-result': raise TimeoutError('simulated disconnection')
            return base(server,token,device,endpoint,payload)
        with patch.object(w,'launch_runtime',return_value='session'),patch.object(w,'api',side_effect=outage):
            self.assertEqual(w.poll(self.server,'test-token','machine',self.folder)[0]['result'],'error')
        path=w.journal_path(self.folder,'job')
        self.assertEqual(json.loads(path.read_text())['phase'],'report_pending')
        with patch.object(w,'herdr',return_value={'result':{'agent':{'status':'idle'}}}):
            w.reconcile(self.server,'test-token','machine',self.folder)
        self.assertEqual(len(self.state['launch_results']),1)
        self.assertEqual(len(self.state['observations']),1)
        self.assertEqual(json.loads(path.read_text())['phase'],'reported')
    def test_unreachable_agent_reports_without_fabricating_completion(self):
        with patch.object(w,'launch_runtime',return_value='session'):
            w.poll(self.server,'test-token','machine',self.folder)
        with patch.object(w,'herdr',side_effect=TimeoutError('offline')):
            w.reconcile(self.server,'test-token','machine',self.folder)
        self.assertEqual(self.state['observations'][-1]['activity'],'unreachable')
        self.assertIsNone(self.state['observations'][-1]['session_ref'])
    def test_rejects_invalid_machine_token_over_http(self):
        from urllib.error import HTTPError
        with self.assertRaises(HTTPError) as captured:
            w.api(self.server,'wrong','machine','inbox')
        self.assertEqual(captured.exception.code,403)

if __name__=='__main__':unittest.main()
