"""Runs that build a commit other than main's own restore caches and do not save them.

GitHub scopes Actions caches by the ref a run was started on: a run started on main that checks out and builds another
commit would write entries main's later runs (releases, the Nightly, the SDK) restore. So every job that
can check out code other than its run's own commit either saves no cache or saves only under a condition that holds for
main's own commit. A job "can check out other code" when a checkout's `ref:` is anything but the run's own commit, or
a step fetches, switches or adds a worktree with git. Each such job with a cache step that can save is classified
below; a new one fails `test_every_job_that_builds_other_code_and_can_save_is_classified` until it is.

This reads the workflow files only: code a job builds could still call the cache service itself, which no workflow
setting stops.
"""
import re
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools/ci/vendor/pyyaml-6.0.3.zip"))
import yaml  # noqa: E402
WORKFLOWS = ROOT / ".github/workflows"
# On Windows the bash on PATH is Git's; a bare "bash" there would be WSL's (tests/test_pr_ready.py).
# Checkout refs that are the run's own commit.
OWN_COMMIT = {None, "${{ github.sha }}"}
# `git -C dir fetch`, `git -c k=v checkout`: options that take a separate value are skipped with it.
OTHER_CODE = re.compile(r"\bgit\s+(?:(?:-[Cc]\s+\S+|-\S+)\s+)*"
                        r"(?:fetch|clone|checkout|switch|worktree|reset|pull|merge|cherry-pick|am)\b"
                        r"|\bgh\s+pr\s+checkout\b")


def workflows():
    for path in sorted([*WORKFLOWS.glob("*.yml"), *WORKFLOWS.glob("*.yaml")]):
        yield path.name, yaml.safe_load(path.read_text())


def action(step):
    return step.get("uses", "").split("@")[0]


def can_save(step):
    """Whether a step can write a cache entry: False, True, or the condition it saves under."""
    name, given = action(step), step.get("with", {}) or {}
    if name == "Swatinem/rust-cache":
        return given.get("save-if", True)
    if name in ("actions/cache", "actions/cache/save"):
        return step.get("if", True)
    if name == "useblacksmith/stickydisk":
        return given.get("commit", True)
    if name == "actions/cache/restore":
        return False
    # Any other cache action (a compiler cache's, say) is taken to save.
    if "cache" in name.lower():
        return True
    # setup-python, setup-java, setup-node and the like: a `cache` input saves at the end of the job.
    return any("cache" in key for key in given) or False


def other_code(job):
    for step in job.get("steps", []):
        given = step.get("with", {}) or {}
        if action(step) == "actions/checkout" and (given.get("ref") not in OWN_COMMIT or "repository" in given):
            return True
        if OTHER_CODE.search(str(step.get("run", ""))):
            return True
    return False


def saving_jobs_with_other_code():
    for name, workflow in workflows():
        for job_name, job in workflow["jobs"].items():
            saves = [step for step in job.get("steps", []) if can_save(step) is not False]
            if saves and other_code(job):
                yield f"{name} {job_name}", job, saves


def step(job, step_id):
    return next(s for s in job["steps"] if s.get("id") == step_id)


class CacheSaveTests(unittest.TestCase):
    # How each job that can build other code keeps its saves to main's own commit.
    CLASSIFIED = {
        # Its `ref` is the caller's: saves only when the caller says `trusted` (see the callers' test below).
        "game-build.yml build",
        # The head is the plan's checkout of the run's own commit; saves only on main: pinned by
        # tests/test_ci_runners.py HostedParityTests.test_only_mains_runs_save.
        "parity-hosted.yml run",
    }

    def test_every_job_that_builds_other_code_and_can_save_is_classified(self):
        found = {where for where, _, _ in saving_jobs_with_other_code()}
        self.assertEqual(found, self.CLASSIFIED)

    def test_the_game_build_saves_only_for_a_trusted_caller_and_no_caller_trusts_another_commit(self):
        job = yaml.safe_load((WORKFLOWS / "game-build.yml").read_text())["jobs"]["build"]
        caches = [s for s in job["steps"] if can_save(s) is not False]
        self.assertEqual([can_save(s) for s in caches], ["${{ inputs.trusted }}"])
        callers = 0
        for name, workflow in workflows():
            for job_name, caller in workflow["jobs"].items():
                if caller.get("uses") == "./.github/workflows/game-build.yml":
                    callers += 1
                    given = caller.get("with", {})
                    # Only a build of the run's own commit may be trusted; the Nightly's set never is.
                    if given.get("trusted", False) is not False:
                        self.assertEqual(given["ref"], "${{ github.sha }}", f"{name} {job_name}")
        self.assertGreater(callers, 0)
        nightly = yaml.safe_load((WORKFLOWS / "nightly-build.yml").read_text())["jobs"]["build"]
        self.assertIs(nightly["with"]["trusted"], False)


if __name__ == "__main__":
    unittest.main()
