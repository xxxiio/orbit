#!/usr/bin/env python3
"""End-to-end tests for the GitHub-hosted root installer (M27 Workstream B).

The tests serve a real release archive produced by scripts/package-release.sh
over a local HTTP server, then run the POSIX installer with test-only URL
overrides. No external network access is used.
"""

import hashlib
import json
import os
import shutil
import subprocess
import tempfile
import threading
import unittest
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
INSTALLER = os.path.join(REPO_ROOT, "install.sh")
PACKAGE_RELEASE = os.path.join(REPO_ROOT, "scripts", "package-release.sh")

VERSION = "0.3.0"
TARGET = "x86_64-unknown-linux-gnu"
ARCHIVE_NAME = "orbit-%s-%s.tar.gz" % (VERSION, TARGET)


def stub_binary(version):
    return (
        "#!/bin/sh\n"
        'if [ "${1:-}" = "--version" ]; then\n'
        '  echo "orbit %s"\n'
        "  exit 0\n"
        "fi\n"
        'echo "orbit-stub %s"\n'
        "exit 0\n"
    ) % (version, version)


def make_handler(partial_archive):
    class Handler(SimpleHTTPRequestHandler):
        requests = []

        def log_message(self, fmt, *args):  # keep test output quiet
            pass

        def do_GET(self):
            Handler.requests.append(self.path)
            if partial_archive and self.path.endswith(".tar.gz"):
                self._send_partial()
                return
            super().do_GET()

        def do_HEAD(self):
            Handler.requests.append(self.path)
            super().do_HEAD()

        def _send_partial(self):
            target = self.translate_path(self.path)
            with open(target, "rb") as handle:
                data = handle.read()
            self.send_response(200)
            self.send_header("Content-Type", "application/gzip")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data[: max(1, len(data) // 2)])
            self.wfile.flush()
            self.close_connection = True

    return Handler


class Site:
    def __init__(self, httpd, thread, handler, serve_dir):
        self.httpd = httpd
        self.thread = thread
        self.handler = handler
        self.serve_dir = serve_dir

    def url(self, path):
        return "http://127.0.0.1:%d%s" % (self.httpd.server_address[1], path)

    def close(self):
        self.httpd.shutdown()
        self.httpd.server_close()
        self.thread.join(timeout=5)


class GithubInstallerTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="orbit-github-installer-")
        self.home = os.path.join(self.tmp, "home")
        self.tmpdir = os.path.join(self.tmp, "tmp")
        os.makedirs(self.home)
        os.makedirs(self.tmpdir)
        self.prefix = os.path.join(self.tmp, "prefix")
        self.sh = shutil.which("sh") or "/bin/sh"
        self.sanitized_bin = os.path.join(self.tmp, "sanitized-bin")
        self._make_sanitized_path(self.sanitized_bin)
        self.sites = []

    def tearDown(self):
        for site in self.sites:
            site.close()
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _make_sanitized_path(self, dest):
        os.makedirs(dest)
        required = [
            "bash", "sh", "curl", "tar", "gzip", "mktemp", "grep", "sed", "tr", "head",
            "cat", "sort", "uniq", "awk", "install", "cp", "mv", "chmod", "rm",
            "mkdir", "basename", "dirname", "uname", "env",
        ]
        for tool in required:
            source = shutil.which(tool)
            self.assertIsNotNone(source, "host is missing required tool: %s" % tool)
            os.symlink(source, os.path.join(dest, tool))
        for tool in ("sha256sum", "shasum"):
            source = shutil.which(tool)
            if source is not None:
                os.symlink(source, os.path.join(dest, tool))
        self.assertTrue(
            os.path.lexists(os.path.join(dest, "sha256sum"))
            or os.path.lexists(os.path.join(dest, "shasum")),
            "host needs sha256sum or shasum",
        )

    def _build_release(self, version):
        bindir = os.path.join(self.tmp, "build", version)
        os.makedirs(bindir)
        orbit = os.path.join(bindir, "orbit")
        with open(orbit, "w", encoding="utf-8") as handle:
            handle.write(stub_binary(version))
        os.chmod(orbit, 0o755)

        outdir = os.path.join(self.tmp, "dist", version)
        os.makedirs(outdir)
        result = subprocess.run(
            [
                "bash", PACKAGE_RELEASE, "--from-dir", bindir,
                "--output", outdir, "--version", version,
                "--target", TARGET, "--components", "cli",
            ],
            capture_output=True, text=True, timeout=60,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        archive = os.path.join(outdir, "orbit-%s-%s.tar.gz" % (version, TARGET))
        sha = archive + ".sha256"
        self.assertTrue(os.path.isfile(archive))
        self.assertTrue(os.path.isfile(sha))
        return archive, sha

    def _start_site(
        self,
        versions=(VERSION,),
        latest_tag="v" + VERSION,
        prerelease=False,
        draft=False,
        partial_archive=False,
    ):
        serve = os.path.join(self.tmp, "serve")
        for version in versions:
            archive, sha = self._build_release(version)
            tag = "v" + version
            asset_dir = os.path.join(serve, "download", tag)
            os.makedirs(asset_dir, exist_ok=True)
            shutil.copyfile(archive, os.path.join(asset_dir, os.path.basename(archive)))
            shutil.copyfile(sha, os.path.join(asset_dir, os.path.basename(sha)))

            # The public installer fetches its helper scripts from the same
            # release asset namespace, never from raw source hosting.
            shutil.copyfile(
                os.path.join(REPO_ROOT, "scripts", "install.sh"),
                os.path.join(asset_dir, "orbit-install-core.sh"),
            )
            shutil.copyfile(
                os.path.join(REPO_ROOT, "scripts", "verify-release.sh"),
                os.path.join(asset_dir, "orbit-verify-release.sh"),
            )

        os.makedirs(os.path.join(serve, "api", "releases"), exist_ok=True)
        handler_cls = make_handler(partial_archive)
        httpd = ThreadingHTTPServer(
            ("127.0.0.1", 0), partial(handler_cls, directory=serve)
        )
        thread = threading.Thread(target=httpd.serve_forever, daemon=True)
        thread.start()
        site = Site(httpd, thread, handler_cls, serve)
        self.sites.append(site)

        latest_version = latest_tag.lstrip("v")
        latest_archive = "orbit-%s-%s.tar.gz" % (latest_version, TARGET)
        latest = {
            "tag_name": latest_tag,
            "prerelease": prerelease,
            "draft": draft,
            "assets": [
                {
                    "name": latest_archive,
                    "browser_download_url": "%s/%s/%s"
                    % (site.url("/download"), latest_tag, latest_archive),
                },
                {
                    "name": latest_archive + ".sha256",
                    "browser_download_url": "%s/%s/%s.sha256"
                    % (site.url("/download"), latest_tag, latest_archive),
                },
            ],
        }
        with open(
            os.path.join(serve, "api", "releases", "latest"), "w", encoding="utf-8"
        ) as handle:
            json.dump(latest, handle)
        return site

    def _env(self, site, target=TARGET):
        return {
            "HOME": self.home,
            "TMPDIR": self.tmpdir,
            "PATH": self.sanitized_bin,
            "LANG": "C",
            "LC_ALL": "C",
            "ORBIT_TARGET": target,
            "ORBIT_REPO": "example/orbit",
            "ORBIT_API_BASE": site.url("/api"),
            "ORBIT_DOWNLOAD_BASE": site.url("/download"),
        }

    def _run(self, site, args, target=TARGET, pipe=False):
        env = self._env(site, target=target)
        if pipe:
            with open(INSTALLER, encoding="utf-8") as handle:
                script = handle.read()
            return subprocess.run(
                [self.sh, "-s", "--"] + args,
                input=script, env=env, capture_output=True, text=True, timeout=60,
            )
        return subprocess.run(
            [self.sh, INSTALLER] + args,
            env=env, capture_output=True, text=True, timeout=60,
        )

    @staticmethod
    def _output(result):
        return (result.stdout or "") + (result.stderr or "")

    def _seed_existing_install(self):
        os.makedirs(os.path.join(self.prefix, "bin"))
        os.makedirs(os.path.join(self.prefix, "share", "orbit"))
        old = os.path.join(self.prefix, "bin", "orbit")
        with open(old, "w", encoding="utf-8") as handle:
            handle.write("#!/bin/sh\necho old-orbit\n")
        os.chmod(old, 0o755)
        with open(
            os.path.join(self.prefix, "share", "orbit", "installed-binaries.txt"),
            "w",
            encoding="utf-8",
        ) as handle:
            handle.write("orbit\n")

    def _assert_existing_untouched(self):
        result = subprocess.run(
            [os.path.join(self.prefix, "bin", "orbit")],
            capture_output=True, text=True,
        )
        self.assertEqual(result.stdout.strip(), "old-orbit")
        with open(
            os.path.join(self.prefix, "share", "orbit", "installed-binaries.txt"),
            encoding="utf-8",
        ) as handle:
            self.assertEqual(handle.read(), "orbit\n")

    def test_latest_stable_install_piped(self):
        site = self._start_site()
        result = self._run(site, ["--prefix", self.prefix], pipe=True)
        self.assertEqual(result.returncode, 0, self._output(result))

        orbit = os.path.join(self.prefix, "bin", "orbit")
        self.assertTrue(os.path.isfile(orbit))
        self.assertTrue(os.access(orbit, os.X_OK))
        self.assertFalse(os.path.exists(os.path.join(self.prefix, "bin", "orbit-mcp")))
        with open(
            os.path.join(self.prefix, "share", "orbit", "installed-binaries.txt"),
            encoding="utf-8",
        ) as handle:
            self.assertEqual(handle.read(), "orbit\n")

        version = subprocess.run([orbit, "--version"], capture_output=True, text=True)
        self.assertIn("orbit %s" % VERSION, version.stdout)
        self.assertIn("not publisher authenticity", result.stdout)
        self.assertIn("Add %s/bin to PATH" % self.prefix, result.stdout)
        self.assertTrue(
            any(path.endswith("/orbit-install-core.sh") for path in site.handler.requests),
            site.handler.requests,
        )
        self.assertTrue(
            any(path.endswith("/orbit-verify-release.sh") for path in site.handler.requests),
            site.handler.requests,
        )
        self.assertFalse(
            any("/raw/" in path for path in site.handler.requests),
            site.handler.requests,
        )

        # The sanitized PATH contains no cargo or python.
        probe = subprocess.run(
            [self.sh, "-c", "command -v cargo; command -v python3"],
            env=self._env(site), capture_output=True, text=True,
        )
        self.assertEqual(probe.stdout.strip(), "")
        self.assertEqual(os.listdir(self.tmpdir), [], "installer left temp files behind")

    def test_explicit_version_install(self):
        site = self._start_site(latest_tag="v0.4.0")
        result = self._run(site, ["--version", VERSION, "--prefix", self.prefix])
        self.assertEqual(result.returncode, 0, self._output(result))
        self.assertTrue(os.path.isfile(os.path.join(self.prefix, "bin", "orbit")))
        self.assertTrue(
            any("/download/v%s/" % VERSION in path for path in site.handler.requests),
            site.handler.requests,
        )
        version = subprocess.run(
            [os.path.join(self.prefix, "bin", "orbit"), "--version"],
            capture_output=True, text=True,
        )
        self.assertIn("orbit %s" % VERSION, version.stdout)

    def test_explicit_prerelease_warns(self):
        prerelease = "0.3.0-rc.1"
        site = self._start_site(versions=(prerelease,), latest_tag="v0.3.0")
        result = self._run(
            site, ["--version", prerelease, "--prefix", self.prefix]
        )
        self.assertEqual(result.returncode, 0, self._output(result))
        self.assertIn("prerelease", self._output(result).lower())
        installed = subprocess.run(
            [os.path.join(self.prefix, "bin", "orbit"), "--version"],
            capture_output=True, text=True,
        )
        self.assertIn("orbit %s" % prerelease, installed.stdout)

    def test_unsupported_target_rejected(self):
        site = self._start_site()
        result = self._run(
            site, ["--prefix", self.prefix], target="riscv64-unknown-linux-gnu"
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("unsupported target", self._output(result))
        self.assertFalse(os.path.exists(os.path.join(self.prefix, "bin", "orbit")))
        self.assertEqual(site.handler.requests, [])

    def test_prerelease_as_latest_rejected(self):
        site = self._start_site(latest_tag="v0.4.0", prerelease=True)
        result = self._run(site, ["--prefix", self.prefix])
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("prerelease", self._output(result).lower())
        self.assertFalse(os.path.exists(os.path.join(self.prefix, "bin", "orbit")))
        self.assertFalse(
            any("/download/" in path for path in site.handler.requests),
            site.handler.requests,
        )

    def test_checksum_mismatch_preserves_existing_install(self):
        site = self._start_site()
        sha_path = os.path.join(
            site.serve_dir, "download", "v" + VERSION, ARCHIVE_NAME + ".sha256"
        )
        with open(sha_path, "w", encoding="utf-8") as handle:
            handle.write("0" * 64 + "  " + ARCHIVE_NAME + "\n")
        self._seed_existing_install()

        result = self._run(site, ["--prefix", self.prefix])
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("checksum", self._output(result).lower())
        self._assert_existing_untouched()

    def test_truncated_archive_preserves_existing_install(self):
        site = self._start_site()
        archive_path = os.path.join(
            site.serve_dir, "download", "v" + VERSION, ARCHIVE_NAME
        )
        with open(archive_path, "rb") as handle:
            data = handle.read()
        truncated = data[: max(1, len(data) // 3)]
        with open(archive_path, "wb") as handle:
            handle.write(truncated)
        with open(archive_path + ".sha256", "w", encoding="utf-8") as handle:
            handle.write(
                hashlib.sha256(truncated).hexdigest() + "  " + ARCHIVE_NAME + "\n"
            )
        self._seed_existing_install()

        result = self._run(site, ["--prefix", self.prefix])
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("archive", self._output(result).lower())
        self._assert_existing_untouched()

    def test_partial_download_preserves_existing_install(self):
        site = self._start_site(partial_archive=True)
        self._seed_existing_install()

        result = self._run(site, ["--prefix", self.prefix])
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("download", self._output(result).lower())
        self._assert_existing_untouched()
        self.assertEqual(os.listdir(self.tmpdir), [], "installer left temp files behind")

    def test_dry_run_prints_resolution_without_downloading(self):
        site = self._start_site()

        explicit = self._run(
            site,
            ["--version", "v" + VERSION, "--prefix", self.prefix, "--dry-run"],
        )
        self.assertEqual(explicit.returncode, 0, self._output(explicit))
        self.assertIn("Tag: v%s" % VERSION, explicit.stdout)
        self.assertIn("Target: %s" % TARGET, explicit.stdout)
        self.assertIn(
            "Asset URL: %s/download/v%s/%s"
            % (site.url(""), VERSION, ARCHIVE_NAME),
            explicit.stdout,
        )
        self.assertIn("Prefix: %s" % self.prefix, explicit.stdout)
        self.assertFalse(os.path.exists(self.prefix))
        self.assertEqual(site.handler.requests, [])

        latest = self._run(site, ["--prefix", self.prefix, "--dry-run"])
        self.assertEqual(latest.returncode, 0, self._output(latest))
        self.assertIn("Tag: v%s" % VERSION, latest.stdout)
        self.assertFalse(os.path.exists(self.prefix))
        self.assertFalse(
            any(path.endswith(".tar.gz") for path in site.handler.requests),
            site.handler.requests,
        )

    def test_help(self):
        site = self._start_site()
        result = self._run(site, ["--help"])
        self.assertEqual(result.returncode, 0, self._output(result))
        self.assertIn("Usage:", result.stdout)
        self.assertEqual(site.handler.requests, [])


if __name__ == "__main__":
    unittest.main(verbosity=2)
