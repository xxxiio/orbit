import importlib.util
import pathlib
import tempfile
import unittest
from unittest.mock import patch

path=pathlib.Path(__file__).with_name('orbit-authority-offline.py')
spec=importlib.util.spec_from_file_location('bridge',path)
bridge=importlib.util.module_from_spec(spec)
spec.loader.exec_module(bridge)

class OfflineTest(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory()
        self.conn=bridge.open_db(pathlib.Path(self.tmp.name)/'queue.sqlite')
    def tearDown(self):
        self.conn.close();self.tmp.cleanup()
    def test_acceptance_preserves_intent_and_receipt(self):
        bridge.add(self.conn,'project',dict(project_id='p',name='P'), 'c1')
        bridge.add(self.conn,'task',dict(action=dict(type='create',task_id='t',title='T',project_id='p')), 'c2')
        calls=[]
        def mock(base,token,method,route,obj=None):
            calls.append((method,route,obj))
            if method=='GET': return 200,dict(revision=len([v for v in calls if v[0]=='POST']))
            return 200,dict(revision=len([v for v in calls if v[0]=='POST']),duplicate=False)
        with patch.object(bridge,'request',mock):
            self.assertEqual(bridge.sync(self.conn,'http://127.0.0.1:8787','secret'),0)
        self.assertEqual(self.conn.execute("SELECT count(*) FROM pending_commands WHERE state='accepted'").fetchone()[0],2)
        self.assertEqual([v[2]['command_id'] for v in calls if v[0]=='POST'],['c1','c2'])
    def test_conflict_stops_queue(self):
        bridge.add(self.conn,'project',dict(project_id='p',name='P'),'c1')
        bridge.add(self.conn,'project',dict(project_id='q',name='Q'),'c2')
        calls=[]
        def mock(*args):
            calls.append(args)
            return (200,{'revision':1}) if args[2]=='GET' else (409,{'error':'conflict'})
        with patch.object(bridge,'request',mock):
            self.assertEqual(bridge.sync(self.conn,'http://127.0.0.1:8787','secret'),3)
        self.assertEqual(self.conn.execute("SELECT state FROM pending_commands WHERE command_id='c2'").fetchone()[0],'pending')
        self.assertEqual(len([c for c in calls if c[2]=='POST']),1)
    def test_network_errors_retain_commands(self):
        bridge.add(self.conn,'project',dict(project_id='p',name='P'),'c1')
        with patch.object(bridge,'request',return_value=(503,{})):
            self.assertEqual(bridge.sync(self.conn,'http://127.0.0.1:8787','secret'),2)
        self.assertEqual(self.conn.execute("SELECT state FROM pending_commands").fetchone()[0],'pending')
    def test_lost_ack_retry_preserves_original_revision_and_command(self):
        bridge.add(self.conn,'project',dict(project_id='p',name='P'),'stable')
        posted=[]
        def flaky(base,token,method,route,obj=None):
            if method=='GET': return 200,{'revision': 8 if not posted else 9}
            posted.append(dict(obj))
            if len(posted)==1: raise bridge.urllib.error.URLError('connection lost after commit')
            return 200,{'revision':9,'duplicate':True}
        with patch.object(bridge,'request',flaky):
            self.assertEqual(bridge.sync(self.conn,'http://127.0.0.1:8787','secret'),2)
            self.assertEqual(bridge.sync(self.conn,'http://127.0.0.1:8787','secret'),0)
        self.assertEqual(posted[0],posted[1])
        self.assertEqual(posted[0]['expected_revision'],8)
    def test_unsafe_http_rejected(self):
        with self.assertRaises(ValueError):
            bridge.request('http://example.org','token','GET','/v1/authoritative/tasks')


class EvidenceQueueTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory()
        self.conn=bridge.open_db(pathlib.Path(self.tmp.name)/'queue.sqlite')
    def tearDown(self):
        self.conn.close(); self.tmp.cleanup()
    def test_habit_and_metric_commands_share_revision_and_preserve_ids(self):
        cases=[('habit',dict(type='create_habit',habit_id='h1',name='Read',cadence='daily')),
               ('occurrence',dict(type='record_habit',occurrence_id='o1',habit_id='h1',occurred_at_ms=100)),
               ('metric',dict(type='set_metric_target',metric_id='m1',baseline=0,target=10,direction='increasing')),
               ('reading',dict(type='record_metric',metric_id='m1',observed_at_ms=200,value=4))]
        for identifier,action in cases:
            bridge.add(self.conn,'evidence',{'action':action},identifier)
        posted=[]
        def mock(base,token,method,route,obj=None):
            if method=='GET':
                self.assertEqual(route,'/v1/authoritative/evidence')
                return 200,dict(revision=len(posted))
            self.assertEqual(route,'/v1/authoritative/evidence-commands')
            posted.append(obj)
            return 200,dict(revision=len(posted),duplicate=False)
        with patch.object(bridge,'request',mock):
            self.assertEqual(bridge.sync(self.conn,'http://localhost:8787','token'),0)
        self.assertEqual([item['expected_revision'] for item in posted],[0,1,2,3])
        self.assertEqual([item['command_id'] for item in posted],[case[0] for case in cases])

def full_snapshot(revision, fingerprint, projects=None, tasks=None, **overrides):
    payload = dict(revision=revision, history_fingerprint=fingerprint,
                   projects=projects if projects is not None else [],
                   tasks=tasks if tasks is not None else [],
                   habits=[], habit_occurrences=[], metrics=[], metric_observations=[])
    payload.update(overrides)
    return payload

class ReplicaTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory()
        self.conn=bridge.open_db(pathlib.Path(self.tmp.name)/'queue.sqlite')
    def tearDown(self):
        self.conn.close();self.tmp.cleanup()
    def test_refresh_populates_atomic_accepted_views(self):
        routes=[]
        def reply(base,token,method,route,obj=None):
            routes.append(route)
            return 200,{'revision':3,'history_fingerprint':'a'*64,'projects':[{'id':'p','name':'P'}],
                        'tasks':[{'id':'t','title':'T','project_id':'p'}],'habits':[{'id':'h','name':'Habit','cadence':'daily'}],'habit_occurrences':[],'metrics':[],'metric_observations':[]}
        with patch.object(bridge,'request',reply):
            self.assertEqual(bridge.refresh(self.conn,'http://localhost:8787','token'),0)
        self.assertEqual(routes,['/v1/authoritative/snapshot'])
        self.assertEqual(bridge.cached(self.conn,'project')['items'][0]['id'],'p')
        self.assertEqual(bridge.cached(self.conn,'task')['revision'],3)
        self.assertEqual(bridge.cached(self.conn,'habit')['items'][0]['id'],'h')
    def test_incomplete_snapshot_keeps_existing_cache(self):
        with patch.object(bridge,'request',return_value=(200,{'revision':3,'history_fingerprint':'a'*64,'projects':[],'tasks':[],'habits':[],'habit_occurrences':[],'metrics':[],'metric_observations':[]})):
            self.assertEqual(bridge.refresh(self.conn,'http://localhost:8787','token'),0)
        with patch.object(bridge,'request',return_value=(200,{'revision':4,'history_fingerprint':'a'*64,'projects':[]})):
            self.assertEqual(bridge.refresh(self.conn,'http://localhost:8787','token'),2)
        self.assertEqual(bridge.cached(self.conn,'task')['revision'],3)
        self.assertEqual(bridge.cached(self.conn,'habit')['items'],[])
    def test_restored_server_does_not_rewind_accepted_cache(self):
        with patch.object(bridge,'request',return_value=(200,{'revision':8,'history_fingerprint':'a'*64,'projects':[],'tasks':[],'habits':[],'habit_occurrences':[],'metrics':[],'metric_observations':[]})):
            self.assertEqual(bridge.refresh(self.conn,'http://localhost:8787','token'),0)
        with patch.object(bridge,'request',return_value=(200,{'revision':3,'history_fingerprint':'a'*64,'projects':[],'tasks':[],'habits':[],'habit_occurrences':[],'metrics':[],'metric_observations':[]})):
            self.assertEqual(bridge.refresh(self.conn,'http://localhost:8787','token'),3)
        self.assertEqual(bridge.cached(self.conn,'project')['revision'],8)
        self.assertEqual(bridge.cached(self.conn,'task')['revision'],8)
    def test_same_revision_different_history_fails_closed(self):
        with patch.object(bridge,'request',return_value=(200,{'revision':3,'history_fingerprint':'a'*64,'projects':[],'tasks':[],'habits':[],'habit_occurrences':[],'metrics':[],'metric_observations':[]})):
            self.assertEqual(bridge.refresh(self.conn,'http://localhost:8787','token'),0)
        with patch.object(bridge,'request',return_value=(200,{'revision':3,'history_fingerprint':'b'*64,'projects':[{'id':'changed'}],'tasks':[],'habits':[],'habit_occurrences':[],'metrics':[],'metric_observations':[]})):
            self.assertEqual(bridge.refresh(self.conn,'http://localhost:8787','token'),3)
        self.assertEqual(bridge.cached(self.conn,'project')['items'],[])
    def test_fingerprint_missing_rejected_without_cache_loss(self):
        with patch.object(bridge,'request',return_value=(200,{'revision':3,'projects':[],'tasks':[],'habits':[],'habit_occurrences':[],'metrics':[],'metric_observations':[]})):
            self.assertEqual(bridge.refresh(self.conn,'http://localhost:8787','token'),2)
        self.assertIsNone(bridge.cached(self.conn,'project'))
    def test_all_six_collections_are_cached_at_one_revision(self):
        payload={'revision':7,'history_fingerprint':'d'*64,
                 'projects':[{'id':'p'}], 'tasks':[{'id':'t'}],
                 'habits':[{'id':'h'}], 'habit_occurrences':[{'id':'o'}],
                 'metrics':[{'id':'m'}], 'metric_observations':[{'metric_id':'m','value':5}]}
        with patch.object(bridge,'request',return_value=(200,payload)):
            self.assertEqual(bridge.refresh(self.conn,'http://localhost:8787','token'),0)
        for kind, key in [('project','projects'),('task','tasks'),('habit','habits'),
                          ('habit_occurrence','habit_occurrences'),('metric','metrics'),
                          ('metric_observation','metric_observations')]:
            self.assertEqual(bridge.cached(self.conn,kind)['items'],payload[key])
            self.assertEqual(bridge.cached(self.conn,kind)['revision'],7)

    def test_existing_client_cache_schema_upgrades_non_destructively(self):
        # The evidence tables are additive; an existing cache with the old
        # project/task CHECK constraint must still be readable after upgrading.
        self.conn.execute("INSERT INTO accepted_snapshots VALUES ('project',1,'[]','old')")
        self.conn.commit()
        payload={'revision':2,'history_fingerprint':'e'*64,'projects':[], 'tasks':[],
                 'habits':[{'id':'h'}], 'habit_occurrences':[], 'metrics':[], 'metric_observations':[]}
        with patch.object(bridge,'request',return_value=(200,payload)):
            self.assertEqual(bridge.refresh(self.conn,'http://localhost:8787','token'),0)
        self.assertEqual(bridge.cached(self.conn,'habit')['items'],[{'id':'h'}])

    def test_missing_evidence_collection_cannot_erase_accepted_snapshot(self):
        all_data={'revision':3,'history_fingerprint':'f'*64,'projects':[], 'tasks':[],
                  'habits':[{'id':'h'}], 'habit_occurrences':[], 'metrics':[], 'metric_observations':[]}
        with patch.object(bridge,'request',return_value=(200,all_data)):
            self.assertEqual(bridge.refresh(self.conn,'http://localhost:8787','token'),0)
        incomplete=dict(all_data, revision=4)
        del incomplete['metrics']
        with patch.object(bridge,'request',return_value=(200,incomplete)):
            self.assertEqual(bridge.refresh(self.conn,'http://localhost:8787','token'),2)
        self.assertEqual(bridge.cached(self.conn,'habit')['revision'],3)

    def test_history_and_collection_cache_commit_atomically(self):
        with patch.object(bridge,'request',return_value=(200,full_snapshot(3,'c'*64))):
            self.assertEqual(bridge.refresh(self.conn,'http://localhost:8787','token'),0)
        self.assertEqual(self.conn.execute('SELECT revision,fingerprint FROM accepted_history').fetchone(),(3,'c'*64))
    def test_rejects_mixed_server_accounts(self):
        bridge.bind_server(self.conn,'https://one.example')
        with self.assertRaises(ValueError):
            bridge.bind_server(self.conn,'https://two.example')
    def test_pending_changes_not_misrepresented_as_accepted(self):
        bridge.add(self.conn,'project',{'project_id':'p','name':'P'})
        self.assertIsNone(bridge.cached(self.conn,'project'))

if __name__=='__main__': unittest.main()
