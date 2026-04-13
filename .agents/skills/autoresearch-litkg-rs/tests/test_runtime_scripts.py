from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[4]
INIT_RUN = REPO_ROOT / ".agents/skills/autoresearch-litkg-rs/scripts/init_run.py"
RECORD_RESULT = REPO_ROOT / ".agents/skills/autoresearch-litkg-rs/scripts/record_result.py"
RESUME_RUN = REPO_ROOT / ".agents/skills/autoresearch-litkg-rs/scripts/resume_run.py"
NEXT_TRIAL = REPO_ROOT / ".agents/skills/autoresearch-litkg-rs/scripts/next_trial.py"


class RuntimeScriptsTest(unittest.TestCase):
    def run_cmd(self, *args: str, cwd: Path | None = None) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, *args],
            cwd=cwd or REPO_ROOT,
            check=False,
            capture_output=True,
            text=True,
        )

    def test_init_run_creates_brief_results_and_state(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            repo_root = Path(tmp_dir)
            result = self.run_cmd(
                str(INIT_RUN),
                "--tag",
                "test-run",
                "--question",
                "Does the runtime initialize correctly?",
                "--primary-metric",
                "runtime-helper score",
                "--direction",
                "higher",
                "--verify-cmd",
                "python3 -m unittest",
                "--guard-cmd",
                "cargo test",
                "--repo-root",
                str(repo_root),
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            run_dir = repo_root / ".logs/autoresearch/test-run"
            brief_path = run_dir / "brief.md"
            results_path = run_dir / "results.tsv"
            state_path = run_dir / "state.json"

            self.assertTrue(brief_path.is_file())
            self.assertTrue(results_path.is_file())
            self.assertTrue(state_path.is_file())

            brief = brief_path.read_text(encoding="utf-8")
            self.assertIn("## Verify Commands", brief)
            self.assertIn("python3 -m unittest", brief)

            state = json.loads(state_path.read_text(encoding="utf-8"))
            self.assertEqual(state["tag"], "test-run")
            self.assertEqual(state["status"], "initialized")
            self.assertEqual(state["best_experiment_id"], None)
            self.assertEqual(state["counts"]["baseline"], 0)

    def test_init_run_rejects_invalid_tag(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            result = self.run_cmd(
                str(INIT_RUN),
                "--tag",
                "../escape",
                "--question",
                "bad tag",
                "--primary-metric",
                "metric",
                "--repo-root",
                tmp_dir,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("Invalid --tag", result.stderr)

    def test_record_result_updates_best_and_pivot_state(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            repo_root = Path(tmp_dir)
            init = self.run_cmd(
                str(INIT_RUN),
                "--tag",
                "test-run",
                "--question",
                "Does the runtime initialize correctly?",
                "--primary-metric",
                "runtime-helper score",
                "--repo-root",
                str(repo_root),
            )
            self.assertEqual(init.returncode, 0, init.stderr)

            baseline = self.run_cmd(
                str(RECORD_RESULT),
                "--tag",
                "test-run",
                "--experiment-id",
                "00-baseline",
                "--commit",
                "abc1234",
                "--status",
                "baseline",
                "--primary-metric",
                "0",
                "--guardrail-status",
                "pass",
                "--description",
                "baseline",
                "--set-best",
                "--repo-root",
                str(repo_root),
            )
            self.assertEqual(baseline.returncode, 0, baseline.stderr)

            keep = self.run_cmd(
                str(RECORD_RESULT),
                "--tag",
                "test-run",
                "--experiment-id",
                "01-tests",
                "--commit",
                "def5678",
                "--status",
                "keep",
                "--primary-metric",
                "1",
                "--guardrail-status",
                "pass",
                "--description",
                "tests pass",
                "--repo-root",
                str(repo_root),
            )
            self.assertEqual(keep.returncode, 0, keep.stderr)

            state = json.loads(
                (repo_root / ".logs/autoresearch/test-run/state.json").read_text(
                    encoding="utf-8"
                )
            )
            self.assertEqual(state["best_experiment_id"], "01-tests")
            self.assertEqual(state["best_primary_metric"], "1")
            self.assertFalse(state["needs_pivot"])

            for experiment_id in ("02-a", "03-b", "04-c"):
                discard = self.run_cmd(
                    str(RECORD_RESULT),
                    "--tag",
                    "test-run",
                    "--experiment-id",
                    experiment_id,
                    "--commit",
                    "-",
                    "--status",
                    "discard",
                    "--primary-metric",
                    "1",
                    "--guardrail-status",
                    "fail:verify",
                    "--description",
                    "did not help",
                    "--repo-root",
                    str(repo_root),
                )
                self.assertEqual(discard.returncode, 0, discard.stderr)

            state = json.loads(
                (repo_root / ".logs/autoresearch/test-run/state.json").read_text(
                    encoding="utf-8"
                )
            )
            self.assertEqual(state["consecutive_non_keep"], 3)
            self.assertTrue(state["needs_pivot"])
            self.assertEqual(state["status"], "needs-pivot")

    def test_record_result_rejects_duplicate_experiment_ids(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            repo_root = Path(tmp_dir)
            init = self.run_cmd(
                str(INIT_RUN),
                "--tag",
                "test-run",
                "--question",
                "Does the runtime initialize correctly?",
                "--primary-metric",
                "runtime-helper score",
                "--repo-root",
                str(repo_root),
            )
            self.assertEqual(init.returncode, 0, init.stderr)

            first = self.run_cmd(
                str(RECORD_RESULT),
                "--tag",
                "test-run",
                "--experiment-id",
                "00-baseline",
                "--commit",
                "abc1234",
                "--status",
                "baseline",
                "--primary-metric",
                "0",
                "--guardrail-status",
                "pass",
                "--description",
                "baseline",
                "--repo-root",
                str(repo_root),
            )
            self.assertEqual(first.returncode, 0, first.stderr)

            duplicate = self.run_cmd(
                str(RECORD_RESULT),
                "--tag",
                "test-run",
                "--experiment-id",
                "00-baseline",
                "--commit",
                "def5678",
                "--status",
                "discard",
                "--primary-metric",
                "0",
                "--guardrail-status",
                "fail:verify",
                "--description",
                "duplicate",
                "--repo-root",
                str(repo_root),
            )
            self.assertNotEqual(duplicate.returncode, 0)
            self.assertIn("already recorded", duplicate.stderr)

    def test_resume_run_summarizes_state_and_recent_results(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            repo_root = Path(tmp_dir)
            init = self.run_cmd(
                str(INIT_RUN),
                "--tag",
                "test-run",
                "--question",
                "Does the runtime initialize correctly?",
                "--primary-metric",
                "runtime-helper score",
                "--repo-root",
                str(repo_root),
            )
            self.assertEqual(init.returncode, 0, init.stderr)

            baseline = self.run_cmd(
                str(RECORD_RESULT),
                "--tag",
                "test-run",
                "--experiment-id",
                "00-baseline",
                "--commit",
                "abc1234",
                "--status",
                "baseline",
                "--primary-metric",
                "0",
                "--guardrail-status",
                "pass",
                "--description",
                "baseline",
                "--set-best",
                "--repo-root",
                str(repo_root),
            )
            self.assertEqual(baseline.returncode, 0, baseline.stderr)

            keep = self.run_cmd(
                str(RECORD_RESULT),
                "--tag",
                "test-run",
                "--experiment-id",
                "01-tests",
                "--commit",
                "def5678",
                "--status",
                "keep",
                "--primary-metric",
                "1",
                "--guardrail-status",
                "pass",
                "--description",
                "tests pass",
                "--repo-root",
                str(repo_root),
            )
            self.assertEqual(keep.returncode, 0, keep.stderr)

            resume = self.run_cmd(
                str(RESUME_RUN),
                "--tag",
                "test-run",
                "--repo-root",
                str(repo_root),
            )
            self.assertEqual(resume.returncode, 0, resume.stderr)
            self.assertIn("Run tag: test-run", resume.stdout)
            self.assertIn("Recommended action: continue from the winner branch", resume.stdout)
            self.assertIn("01-tests [keep]", resume.stdout)

            resume_json = self.run_cmd(
                str(RESUME_RUN),
                "--tag",
                "test-run",
                "--repo-root",
                str(repo_root),
                "--json",
            )
            self.assertEqual(resume_json.returncode, 0, resume_json.stderr)
            payload = json.loads(resume_json.stdout)
            self.assertEqual(payload["best_experiment_id"], "01-tests")
            self.assertFalse(payload["needs_pivot"])

    def test_next_trial_allocates_incrementing_ids_and_branch_names(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            repo_root = Path(tmp_dir)
            init = self.run_cmd(
                str(INIT_RUN),
                "--tag",
                "test-run",
                "--question",
                "Does the runtime initialize correctly?",
                "--primary-metric",
                "runtime-helper score",
                "--repo-root",
                str(repo_root),
            )
            self.assertEqual(init.returncode, 0, init.stderr)

            baseline = self.run_cmd(
                str(RECORD_RESULT),
                "--tag",
                "test-run",
                "--experiment-id",
                "00-baseline",
                "--commit",
                "abc1234",
                "--status",
                "baseline",
                "--primary-metric",
                "0",
                "--guardrail-status",
                "pass",
                "--description",
                "baseline",
                "--repo-root",
                str(repo_root),
            )
            self.assertEqual(baseline.returncode, 0, baseline.stderr)

            suggestion = self.run_cmd(
                str(NEXT_TRIAL),
                "--tag",
                "test-run",
                "--slug",
                "resume-helper",
                "--repo-root",
                str(repo_root),
            )
            self.assertEqual(suggestion.returncode, 0, suggestion.stderr)
            self.assertIn("Next experiment id: 01-resume-helper", suggestion.stdout)
            self.assertIn(
                "Suggested branch: codex/autoresearch-test-run-trial-01",
                suggestion.stdout,
            )

    def test_next_trial_reflects_pivot_requirement(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            repo_root = Path(tmp_dir)
            init = self.run_cmd(
                str(INIT_RUN),
                "--tag",
                "test-run",
                "--question",
                "Does the runtime initialize correctly?",
                "--primary-metric",
                "runtime-helper score",
                "--repo-root",
                str(repo_root),
            )
            self.assertEqual(init.returncode, 0, init.stderr)

            baseline = self.run_cmd(
                str(RECORD_RESULT),
                "--tag",
                "test-run",
                "--experiment-id",
                "00-baseline",
                "--commit",
                "abc1234",
                "--status",
                "baseline",
                "--primary-metric",
                "0",
                "--guardrail-status",
                "pass",
                "--description",
                "baseline",
                "--repo-root",
                str(repo_root),
            )
            self.assertEqual(baseline.returncode, 0, baseline.stderr)

            for experiment_id in ("01-a", "02-b", "03-c"):
                discard = self.run_cmd(
                    str(RECORD_RESULT),
                    "--tag",
                    "test-run",
                    "--experiment-id",
                    experiment_id,
                    "--commit",
                    "-",
                    "--status",
                    "discard",
                    "--primary-metric",
                    "0",
                    "--guardrail-status",
                    "fail:verify",
                    "--description",
                    "did not help",
                    "--repo-root",
                    str(repo_root),
                )
                self.assertEqual(discard.returncode, 0, discard.stderr)

            suggestion = self.run_cmd(
                str(NEXT_TRIAL),
                "--tag",
                "test-run",
                "--repo-root",
                str(repo_root),
                "--json",
            )
            self.assertEqual(suggestion.returncode, 0, suggestion.stderr)
            payload = json.loads(suggestion.stdout)
            self.assertTrue(payload["needs_pivot"])
            self.assertEqual(payload["recommended_action"], "pivot before starting this trial")
            self.assertEqual(payload["experiment_id"], "04-candidate")


if __name__ == "__main__":
    unittest.main()
