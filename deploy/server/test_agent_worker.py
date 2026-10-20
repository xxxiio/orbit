import importlib.util
import unittest
from pathlib import Path
from unittest.mock import patch
spec=importlib.util.spec_from_file_location('agentworker',Path(__file__).with_name('agent-worker.py'))
a=importlib.util.module_from_spec(spec)
spec.loader.exec_module(a)

class WorkerTests(unittest.TestCase):
    def proposal(self,intent,target='object-1',payload=None):
        return {'id':42,'intent':intent,'target':target,'payload':payload or {}}
    def test_all_canonical_domains(self):
        cases=[
            ('task-create',{'title':'Review'},'task',{'type':'create','task_id':'remote-proposal-42','title':'Review','project_id':'object-1'}),
            ('task-update',{'status':'in_progress'},'task',{'type':'change_status','task_id':'object-1','status':'in_progress'}),
            ('habit-record',{'at_ms':12},'evidence',{'type':'record_habit','occurrence_id':'remote-proposal-habit-42','habit_id':'object-1','occurred_at_ms':12}),
            ('metric-observe',{'at_ms':20,'value':-10},'evidence',{'type':'record_metric','metric_id':'object-1','observed_at_ms':20,'value':-10}),
        ]
        for intent,payload,kind,action in cases:
            with self.subTest(intent=intent):
                self.assertEqual(a.command_for(self.proposal(intent,payload=payload)),(kind,'approved-proposal-42',action))
    def test_invalid_payloads_fail_closed(self):
        bad=[('task-create',{'title':'ok','shell':'rm'}),('task-update',{'status':'in-progress'}),('habit-record',{'at_ms':True}),('metric-observe',{'at_ms':1,'value':True}),('metric-observe',{'at_ms':1,'value':2**64}),('shell',{'command':'rm'})]
        for intent,payload in bad:
            with self.subTest(intent=intent,payload=payload),self.assertRaises(ValueError):
                a.command_for(self.proposal(intent,payload=payload))
    def test_missing_stable_id_rejected(self):
        with self.assertRaises(ValueError): a.command_for({'intent':'task-create','target':'p','payload':{'title':'T'}})
    def test_execute_uses_canonical_endpoints_and_revision(self):
        requests=[]
        def fake_request(server,token,path,method='POST',data=None,device=None):
            requests.append((path,method,data))
            if method=='GET': return {'revision':7}
            return {'revision':8,'duplicate':False}
        with patch.object(a,'request',side_effect=fake_request):
            r=a.execute(self.proposal('task-update',payload={'status':'done'}),'https://example.org','secret','worker-1')
        self.assertEqual(r['revision'],8)
        self.assertEqual(requests[0][:2],('/v1/authoritative/snapshot','GET'))
        self.assertEqual(requests[1][0],'/v1/authoritative/task-commands')
        self.assertEqual(requests[1][2]['expected_revision'],7)
        self.assertEqual(requests[1][2]['command_id'],'approved-proposal-42')
    def test_repeated_translation_retains_identity_and_action(self):
        proposal=self.proposal('task-create',payload={'title':'Review'})
        self.assertEqual(a.command_for(proposal),a.command_for(proposal))
        # A new expected revision changes the full server idempotency payload.
        # Expired leases must be manually reconciled, never auto-reclaimed.
    def test_once_settles_rejected_command_as_failure(self):
        class Args: server='https://host'; token='token'; device='worker-1'
        calls=[]
        def fake_request(server,token,path,method='POST',data=None,device=None):
            calls.append((path,data))
            if path.endswith('/claim'): return self.proposal('task-update',payload={'status':'done'})
            if path.endswith('/snapshot'): return {'revision':9}
            if path.endswith('/task-commands'): raise urllib.error.HTTPError('https://host',409,'conflict',{},None)
            return None
        import urllib.error
        with patch.object(a,'request',side_effect=fake_request): self.assertTrue(a.once(Args))
        self.assertFalse(calls[-1][1]['succeeded'])

if __name__=='__main__': unittest.main()
