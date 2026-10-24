"""M29 workflow terminology, rich task details and attachment documentation."""
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DOCS = ROOT / "docs"


def text(path):
    return path.read_text(encoding="utf-8")


class WorkflowAndAttachmentDocs(unittest.TestCase):
    def test_task_details_page_covers_model_and_safety(self):
        page = text(DOCS / "guides" / "task-details-and-attachments.md")
        for term in (
            "acceptance criteria",
            "stable identifier",
            "completion timestamp",
            "never completes the task implicitly",
            "SHA-256",
            "attachments/objects",
            "ORBIT_ATTACHMENT_MAX_BYTES",
            "64 MiB",
            "symlink",
            "external viewer",
            "staged",
            "accepted",
            "pending",
            "orbit attachment flush",
            "online-only",
            "already_removed",
            "single-tenant",
            "No server-side garbage collection endpoint",
            "snapshot-first",
            "accepted replica snapshot",
            "live authoritative task-detail route",
            "GET /v1/authoritative/tasks/{id}/acceptance-criteria",
            "requires the server",
            "conflict",
        ):
            self.assertIn(term, page)
        # The snapshot now carries description/priority/labels; the guide must
        # not repeat the old claim that rich reads always use a live route or
        # that rich data is absent from the accepted snapshot projection.
        self.assertNotIn("six-family accepted snapshot projection", page)
        self.assertNotIn("Connected reads use the authoritative task-detail route", page)
        self.assertNotIn("not available in connected mode", page)
        self.assertNotIn("local-authoritative only", page)
        self.assertNotIn("Server-side attachment upload and storage are", page)
        for tool in (
            "task.acceptance.add",
            "task.acceptance.check",
            "task.acceptance.reorder",
            "task.get",
            "task.update",
            "attachment.list",
            "attachment.associate",
        ):
            self.assertIn(tool, page)

    def test_tui_guide_documents_rich_keys_and_default_workflow(self):
        page = text(DOCS / "guides" / "tui-workspace.md")
        for term in (
            "is the default workflow",
            "Session restore",
            "action bar",
            "Ctrl+S",
            "Acceptance criteria",
            "Activity history",
            "Attach a file",
            "Change the selected project's workflow",
        ):
            self.assertIn(term, page)

    def test_backup_guide_describes_three_parts(self):
        page = text(DOCS / "guides" / "backup-and-recovery.md")
        self.assertIn("three things together", page)
        self.assertIn("attachment object store", page)
        self.assertIn("attachments/objects", page)
        self.assertIn("ORBIT_ATTACHMENT_MAX_BYTES", page)
        self.assertNotIn("two things together", page)

    def test_reference_uses_workflow_terminology_without_losing_commands(self):
        cli = text(DOCS / "reference" / "cli.md")
        self.assertIn("## Blueprint (workflow) commands", cli)
        self.assertIn("orbit blueprint bind", cli)
        self.assertNotIn("No core task priority field", cli)

        mcp = text(DOCS / "reference" / "mcp.md")
        for tool in (
            "workflow.apply",
            "workflow.list",
            "project.create",
            "task.acceptance.check",
            "attachment.associate",
        ):
            self.assertIn(tool, mcp)

    def test_index_and_architecture_link_the_new_model(self):
        index = text(DOCS / "README.md")
        self.assertIn("guides/task-details-and-attachments.md", index)
        architecture = text(DOCS / "architecture.md")
        self.assertIn("attachment object store", architecture)

    def test_mcp_reference_documents_connected_rich_tasks_and_local_attachments(self):
        mcp = text(DOCS / "reference" / "mcp.md")
        for term in (
            "Connected rich task tools are advertised",
            "Attachment tools remain local-only",
            "snapshot-first",
            "accepted replica snapshot",
            "disposition",
            "server_state",
            "accepted: false",
            "pending: false",
            "disposition: conflict",
            "authority.conflict_review",
            "authority.flush",
            "never completes the task implicitly",
            "No MCP operation in this group launches an agent job",
        ):
            self.assertIn(term, mcp)
        # The old contract claimed every rich read used a live route and that
        # reads never came from a cache. Task detail is now snapshot-first.
        self.assertNotIn("stale cache", mcp)
        self.assertNotIn("Reads use the authoritative per-task routes", mcp)
        # Rich task tools must be listed in the connected registry section,
        # while attachment tools stay out of it.
        connected = mcp.split("### Connected registry", 1)[1].split("### Local registry", 1)[0]
        for tool in ("task.get", "task.update", "task.acceptance.list", "task.activity"):
            self.assertIn(tool, connected)
        for tool in ("attachment.add", "attachment.list", "attachment.remove"):
            self.assertNotIn(tool, connected)

    def test_tui_guide_documents_connected_rich_details_and_pending_markers(self):
        page = text(DOCS / "guides" / "tui-workspace.md")
        for term in (
            "Rich task details work in connected mode",
            "[pending]",
            "pending commands",
            "Flush pending commands",
            "online-only",
            "attachment garbage collection is not exposed",
            "blocked or conflicted upload",
            "pending count includes both durable queued commands",
            "staged attachment uploads",
            "conflict` row stops automatic retry",
        ):
            self.assertIn(term, page)
        # Staged uploads are part of the header pending sum; the guide must not
        # revive the claim that they are tracked outside it.
        self.assertNotIn("never folded into that accepted count", page)
        self.assertNotIn("tracked separately", page)
        self.assertNotIn("not available in connected mode", page)
        self.assertNotIn("local-authoritative in this build", page)

    def test_cli_documents_connected_attachment_state_and_agent_job_records(self):
        cli = text(DOCS / "reference" / "cli.md")
        for term in (
            "orbit attachment add",
            "orbit attachment flush",
            "accepted",
            "pending",
            "staged",
            "uploads_pending_before",
            "uploads_pending_after",
            "orbit agent complete",
            "orbit agent interrupt",
            "Orbit job\nrecord only",
            "may keep running",
            "runtime_stopped: false",
        ):
            self.assertIn(term, cli)

    def test_architecture_documents_attachment_routes_and_single_tenant_authority(self):
        architecture = text(DOCS / "architecture.md")
        for term in (
            "/v1/authoritative/attachment-import",
            "/v1/authoritative/attachments/{id}/content",
            "metadata\nonly",
            "attachment-staging",
            "single-tenant",
            "no per-project ACL",
            "authority_history.wire",
            "includes each task's description, priority",
            "acceptance-criteria` returns the ordered",
            "uses\ndedicated authenticated routes",
        ):
            self.assertIn(term, architecture)
        # The snapshot carries rich task fields; do not claim the design avoids
        # widening it.
        self.assertNotIn("rather than widening the six-family accepted snapshot", architecture)

    def test_backup_guide_covers_server_store_and_replica_staging(self):
        page = text(DOCS / "guides" / "backup-and-recovery.md")
        for term in (
            "server attachment object store",
            "attachment-staging",
            "unflushed uploads",
        ):
            self.assertIn(term, page)


if __name__ == "__main__":
    unittest.main()
