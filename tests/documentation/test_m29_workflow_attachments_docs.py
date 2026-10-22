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
            "not available in connected mode",
            "external viewer",
        ):
            self.assertIn(term, page)
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


if __name__ == "__main__":
    unittest.main()
