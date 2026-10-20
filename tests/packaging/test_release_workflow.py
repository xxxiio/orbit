"""Static and behavioral checks for the M27 GitHub release workflow.

These tests intentionally read the workflow as text so they run without PyYAML
or actionlint. The release-mode decision script is extracted and executed with
bash to verify draft/prerelease behavior instead of only matching strings.
"""
import os
import pathlib
import re
import subprocess
import tempfile
import textwrap
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / ".github/workflows/release.yml"
VALIDATOR = ROOT / "scripts/release/validate-release-version.sh"

TARGET_RUNNERS = {
    "x86_64-unknown-linux-gnu": "ubuntu-22.04",
    "aarch64-unknown-linux-gnu": "ubuntu-24.04-arm",
    "x86_64-apple-darwin": "macos-15-intel",
    "aarch64-apple-darwin": "macos-14",
}


def job_block(workflow_text, name):
    """Return the text of a top-level job, using indentation as the boundary."""
    lines = workflow_text.splitlines()
    start = None
    for index, line in enumerate(lines):
        if re.fullmatch(rf"  {re.escape(name)}:", line):
            start = index
            break
    if start is None:
        raise AssertionError(f"job not found: {name}")
    end = len(lines)
    for index in range(start + 1, len(lines)):
        if re.match(r"^  [A-Za-z0-9_-]+:\s*$", lines[index]):
            end = index
            break
    return "\n".join(lines[start:end])


def step_run(job_text, step_name):
    """Extract and dedent the `run: |` script for a named step."""
    lines = job_text.splitlines()
    for index, line in enumerate(lines):
        if line.strip() != f"- name: {step_name}":
            continue
        step_indent = len(line) - len(line.lstrip())
        for cursor in range(index + 1, len(lines)):
            current = lines[cursor]
            indentation = len(current) - len(current.lstrip())
            if current.strip() and indentation <= step_indent and current.lstrip().startswith("- "):
                break
            if current.strip() == "run: |":
                body = []
                for body_index in range(cursor + 1, len(lines)):
                    body_line = lines[body_index]
                    if not body_line.strip():
                        body.append("")
                        continue
                    body_indent = len(body_line) - len(body_line.lstrip())
                    if body_indent <= indentation:
                        break
                    body.append(body_line)
                return textwrap.dedent("\n".join(body))
    raise AssertionError(f"run script not found for step: {step_name}")


class ReleaseWorkflowTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.text = WORKFLOW.read_text()
        cls.gate = job_block(cls.text, "gate")
        cls.build = job_block(cls.text, "build")
        cls.release = job_block(cls.text, "release")

    def run_release_mode(self, event_name, tag, publish_input):
        script = step_run(self.release, "Decide release mode")
        with tempfile.TemporaryDirectory() as directory:
            github_env = pathlib.Path(directory) / "github-env"
            env = {
                **os.environ,
                "GITHUB_EVENT_NAME": event_name,
                "TAG": tag,
                "PUBLISH_INPUT": publish_input,
                "GITHUB_ENV": str(github_env),
            }
            probe = script + '\nprintf "%s %s" "$DRAFT" "$PRERELEASE"'
            result = subprocess.run(
                ["bash", "-c", probe], env=env, text=True, capture_output=True
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            return tuple(result.stdout.split())

    def test_triggers(self):
        self.assertRegex(self.text, r"(?m)^on:\s*$")
        self.assertRegex(self.text, r"(?m)^  push:\s*$")
        self.assertRegex(self.text, r"tags:\s*\n\s*-\s*['\"]v\*['\"]")
        self.assertIn("workflow_dispatch:", self.text)
        dispatch = self.text.split("workflow_dispatch:", 1)[1].split("permissions:", 1)[0]
        version_block = dispatch.split("version:", 1)[1].split("publish:", 1)[0]
        publish_block = dispatch.split("publish:", 1)[1]
        self.assertRegex(version_block, r"required:\s*true")
        self.assertRegex(version_block, r"type:\s*string")
        self.assertRegex(publish_block, r"type:\s*boolean")
        self.assertRegex(publish_block, r"default:\s*false")

    def test_gate_runs_on_ubuntu_with_rust_199_and_all_checks(self):
        self.assertRegex(self.gate, r"runs-on:\s*ubuntu-")
        self.assertIn("toolchain: 1.99.0", self.gate)
        self.assertIn("components: rustfmt, clippy", self.gate)
        for command in (
            'scripts/release/validate-release-version.sh "$TAG"',
            "cargo fmt --all -- --check",
            "cargo clippy --workspace --all-targets --locked -- -D warnings",
            "cargo test --workspace --locked",
        ):
            self.assertIn(command, self.gate)

    def test_build_matrix_uses_native_runners(self):
        pairs = re.findall(
            r"-\s+target:\s*(\S+)\s*\n\s+runner:\s*(\S+)", self.build
        )
        self.assertEqual(len(pairs), 4)
        self.assertEqual(dict(pairs), TARGET_RUNNERS)
        self.assertIn("fail-fast: true", self.build)

    def test_canonical_asset_naming_and_packaging_command(self):
        self.assertIn(
            'scripts/package-release.sh --build --target "${{ matrix.target }}" '
            '--version "$VERSION" --components cli,mcp,tui,server',
            self.build,
        )
        self.assertIn('dist/orbit-$VERSION-${{ matrix.target }}.tar.gz', self.build)
        self.assertNotIn("orbit-v$VERSION", self.build)
        self.assertNotIn("orbit-v${{", self.build)
        self.assertIn('scripts/verify-release.sh "$ARCHIVE"', self.build)
        self.assertIn('${{ env.ARCHIVE }}.sha256', self.build)

    def test_permissions_are_least_privilege(self):
        self.assertRegex(self.text, r"(?m)^permissions:\s*\n  contents:\s*read\s*$")
        self.assertRegex(self.build, r"(?m)^      contents:\s*read\s*$")
        self.assertRegex(self.build, r"(?m)^      id-token:\s*write\s*$")
        self.assertRegex(self.build, r"(?m)^      attestations:\s*write\s*$")
        self.assertRegex(self.release, r"(?m)^      contents:\s*write\s*$")
        self.assertRegex(self.release, r"(?m)^    environment:\s*release\s*$")
        self.assertNotRegex(self.release, r"(?m)^      id-token:\s*write\s*$")

    def test_attestation_step_present_in_every_build(self):
        self.assertIn("actions/attest-build-provenance@v2", self.build)
        self.assertRegex(
            self.build, r"subject-path:\s*\$\{\{\s*env\.ARCHIVE\s*\}\}"
        )
        self.assertIn("actions/upload-artifact@v4", self.build)

    def test_release_needs_gate_and_all_builds(self):
        self.assertRegex(self.build, r"(?m)^    needs:\s*gate\s*$")
        self.assertRegex(
            self.release, r"(?m)^    needs:\s*\[\s*gate\s*,\s*build\s*\]\s*$"
        )
        self.assertIn("actions/download-artifact@v4", self.release)
        self.assertIn("Re-verify downloaded archives", self.release)
        self.assertIn('scripts/verify-release.sh "$archive"', self.release)

    def test_tag_push_is_always_a_draft(self):
        self.assertEqual(self.run_release_mode("push", "v0.1.0", "false"), ("1", "0"))
        # A tag push cannot opt into publishing even if a stale input is set.
        self.assertEqual(self.run_release_mode("push", "v0.1.0", "true"), ("1", "0"))
        self.assertIn("--draft", self.release)
        self.assertIn('GH_TOKEN: ${{ github.token }}', self.release)

    def test_workflow_dispatch_publish_requires_explicit_true(self):
        self.assertEqual(
            self.run_release_mode("workflow_dispatch", "v0.1.0", "false"), ("1", "0")
        )
        self.assertEqual(
            self.run_release_mode("workflow_dispatch", "v0.1.0", "true"), ("0", "0")
        )
        self.assertIn(
            "PUBLISH_INPUT: ${{ github.event.inputs.publish || 'false' }}",
            self.release,
        )

    def test_prerelease_tags_are_never_marked_stable(self):
        self.assertIn(r"^v[0-9]+\.[0-9]+\.[0-9]+-", self.release)
        self.assertEqual(
            self.run_release_mode("workflow_dispatch", "v0.2.0-rc.1", "true"),
            ("0", "1"),
        )
        self.assertEqual(
            self.run_release_mode("push", "v0.2.0-rc.1", "false"), ("1", "1")
        )
        self.assertIn("--prerelease", self.release)
        self.assertIn("--prerelease=false", self.release)

    def test_installer_assets_are_staged_from_the_release_tag(self):
        stage = step_run(self.release, "Stage installer assets")
        for asset in (
            "dist/install.sh",
            "dist/orbit-install-core.sh",
            "dist/orbit-verify-release.sh",
        ):
            self.assertIn(asset, stage)
        create = step_run(self.release, "Create or update GitHub release")
        for asset in (
            "dist/install.sh",
            "dist/orbit-install-core.sh",
            "dist/orbit-verify-release.sh",
        ):
            self.assertIn(asset, create)
        self.assertIn("expected 11 release files", create)

    def test_no_unconditional_stable_publish(self):
        script = step_run(self.release, "Decide release mode")
        self.assertIn("DRAFT=1", script)
        self.assertIn(
            'if [[ "$GITHUB_EVENT_NAME" == "workflow_dispatch" && "$PUBLISH_INPUT" == "true" ]]; then',
            script,
        )
        self.assertIn("DRAFT=0", script)


class ValidateReleaseVersionTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.cargo_toml = pathlib.Path(self.tmp.name) / "Cargo.toml"

    def write_cargo(self, version):
        self.cargo_toml.write_text(
            "[workspace]\n"
            'members = []\n'
            "\n"
            "[workspace.package]\n"
            f'version = "{version}"\n'
            "\n"
            "[profile.release]\n"
            "opt-level = 3\n"
        )

    def run_validator(self, *arguments):
        return subprocess.run(
            ["bash", str(VALIDATOR), *arguments],
            text=True,
            capture_output=True,
        )

    def test_accepts_stable_tag(self):
        self.write_cargo("0.1.0")
        result = self.run_validator("v0.1.0", "--cargo-toml", str(self.cargo_toml))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("0.1.0", result.stdout)

    def test_accepts_prerelease_tag(self):
        self.write_cargo("0.2.0-rc.1")
        result = self.run_validator(
            "v0.2.0-rc.1", "--cargo-toml", str(self.cargo_toml)
        )
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_rejects_version_mismatch(self):
        self.write_cargo("0.1.0")
        result = self.run_validator("v0.2.0", "--cargo-toml", str(self.cargo_toml))
        self.assertEqual(result.returncode, 2)
        self.assertIn("does not match", result.stderr)

    def test_rejects_tag_without_leading_v(self):
        self.write_cargo("0.1.0")
        result = self.run_validator("0.1.0", "--cargo-toml", str(self.cargo_toml))
        self.assertEqual(result.returncode, 2)
        self.assertIn("vX.Y.Z", result.stderr)

    def test_rejects_incomplete_tag(self):
        self.write_cargo("0.1.0")
        result = self.run_validator("v0.1", "--cargo-toml", str(self.cargo_toml))
        self.assertEqual(result.returncode, 2)
        self.assertIn("vX.Y.Z", result.stderr)

    def test_rejects_missing_tag(self):
        self.write_cargo("0.1.0")
        result = self.run_validator("--cargo-toml", str(self.cargo_toml))
        self.assertEqual(result.returncode, 2)
        self.assertIn("missing release tag", result.stderr)

    def test_print_version(self):
        self.write_cargo("1.2.3")
        result = self.run_validator(
            "--print-version", "--cargo-toml", str(self.cargo_toml)
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "1.2.3")

    def test_missing_cargo_toml_is_an_error(self):
        result = self.run_validator(
            "v0.1.0", "--cargo-toml", str(pathlib.Path(self.tmp.name) / "missing.toml")
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("Cargo.toml not found", result.stderr)


if __name__ == "__main__":
    unittest.main()
