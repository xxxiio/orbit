#!/usr/bin/env python3
"""Opt-in approved-proposal executor using ONLY canonical Orbit server APIs.

An enrolled worker claims approved intents and submits stable idempotent commands.
The server owns validation, domain changes, and SQLite. No local Orbit DB/CLI.
"""
import argparse
import json
import os
import time
import urllib.error
import urllib.request
from urllib.parse import urlsplit

VALID_STATUSES = {'inbox','planned','ready','in_progress','waiting','blocked','review','deferred','done','cancelled'}

from worker_transport import json_request, validate_server


def request(server, token, path, method='POST', data=None, device=None):
    return json_request(server, token, path, method=method, payload=data,
                        device=device, max_response=65536)

def command_for(proposal):
    """Pure, allowlisted translation from a reviewed intent to a domain action."""
    ident = proposal.get('id')
    intent, target, payload = proposal.get('intent'), proposal.get('target'), proposal.get('payload')
    if type(ident) is not int or ident <= 0 or not isinstance(target,str) or not target.strip() or len(target)>256 or not isinstance(payload,dict):
        raise ValueError('invalid proposal')
    command_id = 'approved-proposal-%d' % ident
    if intent == 'task-create':
        if set(payload)!={'title'} or not isinstance(payload['title'],str) or not 0<len(payload['title'].strip())<=250:
            raise ValueError('invalid task title')
        return 'task',command_id,{'type':'create','task_id':'remote-proposal-%d'%ident,'title':payload['title'],'project_id':target}
    if intent == 'task-update':
        if set(payload)!={'status'} or not isinstance(payload['status'],str) or payload['status'] not in VALID_STATUSES:
            raise ValueError('invalid status change')
        return 'task',command_id,{'type':'change_status','task_id':target,'status':payload['status']}
    if intent == 'habit-record':
        if set(payload)!={'at_ms'} or type(payload['at_ms']) is not int or payload['at_ms']<=0:
            raise ValueError('invalid habit occurrence')
        return 'evidence',command_id,{'type':'record_habit','occurrence_id':'remote-proposal-habit-%d'%ident,'habit_id':target,'occurred_at_ms':payload['at_ms']}
    if intent == 'metric-observe':
        if set(payload)!={'at_ms','value'} or type(payload['at_ms']) is not int or payload['at_ms']<=0 or type(payload['value']) is not int or not -(2**63)<=payload['value']<2**63:
            raise ValueError('invalid metric observation')
        return 'evidence',command_id,{'type':'record_metric','metric_id':target,'observed_at_ms':payload['at_ms'],'value':payload['value']}
    raise ValueError('unsupported mutation intent')

def execute(proposal, server, token, device):
    """Commit an approved intent through canonical server validation.

    Stable IDs identify intentions, but a command receipt also binds its
    submission revision. Expired leases are never auto-retried: after lost
    acknowledgement, an operator must reconcile the accepted server state.
    """
    kind,command_id,action = command_for(proposal)
    snapshot = request(server,token,'/v1/authoritative/snapshot',method='GET',device=device)
    revision = snapshot['revision']
    if type(revision) is not int or revision < 0: raise ValueError('invalid authority revision')
    return request(server,token,'/v1/authoritative/'+kind+'-commands',
        data={'command_id':command_id,'expected_revision':revision,'action':action},device=device)

def once(args):
    p = request(args.server,args.token,'/v1/agent-proposals/claim',device=args.device)
    if p is None: return False
    try:
        result = execute(p,args.server,args.token,args.device)
        succeeded = True
    except (ValueError,KeyError,TypeError,urllib.error.HTTPError,urllib.error.URLError,TimeoutError,json.JSONDecodeError) as exc:
        succeeded = False
        result = {'error':type(exc).__name__,'message':str(exc)[:200]}
    request(args.server,args.token,'/v1/agent-proposals/%d/settle'%int(p['id']),
            data={'succeeded':succeeded,'result':result},device=args.device)
    return True

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--server',required=True)
    parser.add_argument('--device',required=True,help='enrolled worker device identity')
    parser.add_argument('--once',action='store_true')
    parser.add_argument('--interval',type=int,default=30)
    args=parser.parse_args()
    args.token=os.environ.get('ORBIT_SYNC_HTTP_TOKEN','')
    try: validate_server(args.server)
    except ValueError as error: parser.error(str(error))
    if not args.token: parser.error('ORBIT_SYNC_HTTP_TOKEN required')
    if args.interval<5: parser.error('interval must be >=5')
    while True:
        try: once(args)
        except Exception as exc: print('agent worker:',str(exc)[:200],flush=True)
        if args.once: break
        time.sleep(args.interval)

if __name__=='__main__': main()
