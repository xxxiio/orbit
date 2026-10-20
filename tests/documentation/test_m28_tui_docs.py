"""M28 terminal workspace and shared UI contract documentation checks."""
import json
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DOCS = ROOT / "docs"


def text(path):
    return path.read_text(encoding="utf-8")


class M28DocumentationTests(unittest.TestCase):
    def test_tui_guide_covers_navigation_and_troubleshooting(self):
        page = text(DOCS / "guides" / "tui-workspace.md")
        self.assertGreater(len(page), 2500)
        for term in (
            "Home / Today",
            "Inbox",
            "Projects",
            "Agents",
            "g` then `p",
            "Command palette",
            "narrow",
            "stale",
            "generated default",
            "orbit-tui",
        ):
            self.assertIn(term, page)

    def test_blueprint_view_reference_lists_registered_components(self):
        page = text(DOCS / "reference" / "ui-views.md")
        components = (
            "text",
            "list",
            "table",
            "metric",
            "detail",
            "kanban",
            "task_list",
            "timeline",
            "habit_tracker",
            "metric_history",
            "knowledge_refs",
            "agent_jobs",
            "milestone_progress",
        )
        for component in components:
            self.assertIn(f"`{component}`", page)
        for source in (
            "project",
            "tasks",
            "work_items",
            "milestones",
            "knowledge",
            "habits",
            "metrics",
            "agent_jobs",
            "schedule",
        ):
            self.assertIn(f"`{source}`", page)
        self.assertIn("closed", page)

    def test_component_developer_guide_covers_contract_and_flutter(self):
        page = text(DOCS / "developers" / "ui-components.md")
        for term in (
            "ComponentRegistry::builtin",
            "ViewDataSource",
            "ProjectData",
            "WorkspaceData",
            "ResolvedView",
            "SelectionItem",
            "AvailableAction",
            "Future Flutter integration",
            "dynamic libraries",
        ):
            self.assertIn(term, page)

    def test_bundled_manifests_expose_differentiated_views(self):
        expected = {
            "development": {"overview", "board", "milestones", "agents"},
            "research": {"overview", "evidence", "workflow", "references"},
            "learning": {"overview", "lessons", "tasks", "progress"},
            "habits": {"overview", "tracker", "metrics", "goals"},
        }
        for name, view_ids in expected.items():
            blueprint = json.loads(text(ROOT / "blueprints" / f"{name}.json"))
            self.assertGreaterEqual(blueprint["version"], 2)
            ids = {view["id"] for view in blueprint["views"]}
            self.assertEqual(ids, view_ids, name)
            for view in blueprint["views"]:
                self.assertTrue(view["components"], name)
                component_ids = [component["id"] for component in view["components"]]
                self.assertEqual(len(component_ids), len(set(component_ids)), name)

    def test_no_unsafe_manifest_execution_fields(self):
        forbidden = re.compile(r'"(?:command|script|sql|exec|shell|path)"\s*:', re.IGNORECASE)
        for name in ("development", "research", "learning", "habits"):
            body = text(ROOT / "blueprints" / f"{name}.json")
            self.assertIsNone(forbidden.search(body), name)


if __name__ == "__main__":
    unittest.main()
