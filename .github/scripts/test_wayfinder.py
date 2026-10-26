"""Regression checks for the compiled Wayfinder automation contract."""

import json
from pathlib import Path
import re
import unittest


ROOT = Path(__file__).resolve().parents[2]
WORKFLOWS = ROOT / ".github" / "workflows"
STAGES = ("wayfinder-to-spec", "wayfinder-spec", "wayfinder-plan", "wayfinder-delivery")


def source(stage):
    return (WORKFLOWS / f"{stage}.md").read_text()


def lock(stage):
    return (WORKFLOWS / f"{stage}.lock.yml").read_text()


def handlers(stage):
    match = re.search(
        r"^\s+GH_AW_SAFE_OUTPUTS_HANDLER_CONFIG: (.+)$", lock(stage), re.MULTILINE
    )
    if match is None:
        raise AssertionError(f"{stage} has no compiled safe-output configuration")
    return json.loads(json.loads(match.group(1)))


class WayfinderContractTests(unittest.TestCase):
    def test_every_stage_has_reconciliation_not_only_label_events(self):
        for stage in STAGES:
            with self.subTest(stage=stage):
                self.assertIn("schedule: every 30 minutes", source(stage))
                self.assertIn("workflow_dispatch:", source(stage))
                self.assertRegex(lock(stage), r"(?m)^\s+schedule:")
                self.assertRegex(lock(stage), r"(?m)^\s+workflow_dispatch:")
                self.assertIn('target: "*"', source(stage))
                self.assertIn(f"group: {stage}", source(stage))
                self.assertIn("queue: max", lock(stage))

    def test_spec_handoff_can_add_both_labels_in_one_output(self):
        config = handlers("wayfinder-spec")["add_labels"]
        self.assertEqual(set(config["allowed"]), {"specced", "to-tickets"})
        # add-labels max counts labels, not tool calls. One silently drops the queue.
        self.assertGreaterEqual(config["max"], len(config["allowed"]))
        self.assertTrue(config["create_if_missing"])
        self.assertIn("issues: true", source("wayfinder-spec"))
        self.assertIn("pull-requests: false", source("wayfinder-spec"))
        self.assertEqual(config["target"], "*")
        self.assertNotIn("required_labels", config)

    def test_spec_repairs_legacy_markerless_comments_without_reposting(self):
        text = source("wayfinder-spec")
        for heading in (
            "Problem Statement", "Solution", "User Stories",
            "Implementation Decisions", "Testing Decisions", "Out of Scope",
            "Further Notes",
        ):
            self.assertIn(f"## {heading}", text)
        self.assertIn("do not publish another", text)
        self.assertIn("Never requeue an issue labelled `planned`", text)
        self.assertIn("Do not require `wayfinder:map`", text)

    def test_map_gate_requires_closed_decisions_and_excludes_delivery_states(self):
        text = source("wayfinder-to-spec")
        self.assertIn("at least one native wayfinder decision sub-issue", text)
        self.assertIn("sub-issue is closed", text)
        for label in ("to-spec", "specced", "to-tickets", "to-plan", "planned"):
            self.assertIn(f"`{label}`", text)

    def test_plan_accepts_both_queue_labels_for_any_parent(self):
        text = source("wayfinder-plan")
        self.assertIn("legacy alias `to-plan`", text)
        self.assertIn("excluding `planned`", text)
        self.assertIn("Ordinary issues", text)
        config = handlers("wayfinder-plan")
        self.assertEqual(config["replace_label"]["target"], "*")
        self.assertEqual(config["replace_label"]["allowed_add"], ["planned"])
        self.assertEqual(
            set(config["replace_label"]["allowed_remove"]), {"to-tickets", "to-plan"}
        )
        self.assertEqual(config["replace_label"]["required_labels"], ["specced"])
        self.assertIn("ONE label to add and ONE to remove", text)
        self.assertIn("wayfinder-plan:v1", text)
        self.assertIn("blocked_by", text)
        self.assertIn("set `parent`", text)
        self.assertTrue(config["create_issue"]["require_temporary_id"])
        self.assertTrue(config["create_issue"]["deduplicate_by_title"])

    def test_requested_skills_are_installed_declaratively(self):
        for stage, skill in (("wayfinder-spec", "to-spec"), ("wayfinder-plan", "to-tickets")):
            with self.subTest(stage=stage):
                self.assertIn(f"  - .agents/skills/{skill}", source(stage))
                self.assertIn(f"Install frontmatter skill: .agents/skills/{skill}", lock(stage))

    def test_delivery_requests_copilot_review_and_never_clears_it(self):
        config = handlers("wayfinder-delivery")
        self.assertEqual(config["add_reviewer"]["allowed"], ["copilot"])
        self.assertEqual(config["add_reviewer"]["target"], "*")
        self.assertIn("GH_AW_AGENT_TOKEN", lock("wayfinder-delivery"))
        self.assertIn("github-token: ${{ secrets.GH_AW_AGENT_TOKEN }}", source("wayfinder-delivery"))
        self.assertFalse((WORKFLOWS / "clear-copilot-review-requests.yml").exists())
        self.assertIn("pull_request_review:", lock("wayfinder-delivery"))
        self.assertIn(
            "github.event.pull_request.head.repo.full_name == github.repository",
            lock("wayfinder-delivery"),
        )
        self.assertIn("do not silently request forever", source("wayfinder-delivery"))

    def test_merge_requires_current_review_checks_and_resolved_findings(self):
        text = source("wayfinder-delivery")
        for requirement in (
            "`commit_id` equals the current head SHA",
            "copilot-pull-request-reviewer[bot]",
            "all required checks have completed successfully",
            "no unresolved actionable review thread remains",
            "never merge without it",
            "deduplicated `@copilot` follow-up",
            "branch protection and repository merge policy permit the merge",
        ):
            self.assertIn(requirement, text)
        self.assertEqual(
            handlers("wayfinder-delivery")["merge_pull_request"]["allowed_branches"],
            ["copilot/*", "copilot/**"],
        )
        self.assertNotIn("without requesting or performing a code review", text)

    def test_parent_completion_requires_merged_work_not_just_closed_issues(self):
        text = source("wayfinder-delivery")
        self.assertIn("Allow only one active coding ticket per map", text)
        self.assertIn("AND any implementation pull request", text)
        self.assertIn("at least one implementation sub-issue exists", text)
        self.assertIn("a merged pull request satisfying its acceptance criteria", text)
        self.assertIn("abandoned in a closed unmerged pull request", text)


if __name__ == "__main__":
    unittest.main()
