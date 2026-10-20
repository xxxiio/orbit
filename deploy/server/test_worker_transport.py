"""Focused source-level transport invariants for the three optional workers."""
import unittest
from unittest.mock import patch
from worker_transport import json_request, validate_server


class WorkerTransportTests(unittest.TestCase):
    def test_https_and_explicit_loopback(self):
        for base in ('https://orbit.example', 'http://localhost:8787', 'http://127.0.0.1:8787'):
            self.assertEqual(validate_server(base), base)

    def test_no_nonlocal_cleartext_or_credential_base(self):
        for base in ('http://orbit.example', 'http://192.168.1.2:8787',
                     'https://user:pass@orbit.example', 'https://orbit.example/?key=1',
                     'https://orbit.example/subpath', 'https://orbit.example#fragment'):
            with self.subTest(base=base), self.assertRaises(ValueError):
                validate_server(base)

    def test_no_invalid_endpoints(self):
        for path in ('https://evil.example', '//evil.example', '/ok\r\nHeader: forged', '/ok#fragment'):
            with self.subTest(path=path), self.assertRaises(ValueError):
                json_request('https://orbit.example', 'token', path)

    def test_redirect_rejected(self):
        from worker_transport import RejectRedirect
        with self.assertRaises(ValueError):
            RejectRedirect().redirect_request(None, None, 302, 'redirect', {}, 'https://elsewhere')

    def test_request_header_and_json_payload(self):
        class Response:
            def __enter__(self): return self
            def __exit__(self, *a): pass
            def read(self, n): return b'{"ok":true}'
        with patch('worker_transport._OPENER') as opener:
            opener.open.return_value = Response()
            self.assertEqual(json_request('https://orbit.example', 'secret', '/v1/x',
                method='POST', payload={'x': 1}, device='node-1'), {'ok': True})
            request = opener.open.call_args.args[0]
            self.assertEqual(request.get_header('Authorization'), 'Bearer secret')
            self.assertEqual(request.get_header('X-orbit-device-id'), 'node-1')
            self.assertEqual(request.data, b'{"x":1}')

    def test_response_size_bound(self):
        class Response:
            def __enter__(self): return self
            def __exit__(self, *a): pass
            def read(self, n): return b'a' * n
        with patch('worker_transport._OPENER') as opener:
            opener.open.return_value = Response()
            with self.assertRaisesRegex(ValueError, 'oversized'):
                json_request('https://orbit.example', 'secret', '/v1/x', max_response=8)


if __name__ == '__main__': unittest.main()
