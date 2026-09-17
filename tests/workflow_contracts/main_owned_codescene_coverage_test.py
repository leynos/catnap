"""Guard main-owned CodeScene coverage publication."""

from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[2]
PIN = "152d9c4784d0ae5877938a984fe6d1f04d718fd8"


def _steps(name: str, job_name: str) -> list[dict[str, object]]:
    """Return mapping steps from one named workflow job."""
    document = yaml.safe_load((ROOT / ".github/workflows" / name).read_text())
    assert isinstance(document, dict)
    jobs = document["jobs"]
    assert isinstance(jobs, dict)
    job = jobs[job_name]
    assert isinstance(job, dict)
    steps = job["steps"]
    assert isinstance(steps, list)
    return [step for step in steps if isinstance(step, dict)]


def test_pull_requests_keep_coverage_local() -> None:
    """Require the PR lane to use only the local coverage ratchet."""
    source = (ROOT / ".github/workflows/ci.yml").read_text()
    coverage = next(
        step
        for step in _steps("ci.yml", "build-test")
        if step.get("name") == "Test and Measure Coverage"
    )
    assert (
        coverage["uses"]
        == f"leynos/shared-actions/.github/actions/generate-coverage@{PIN}"
    )
    assert coverage["with"]["with-ratchet"] == "true"
    assert "upload-codescene-coverage" not in source
    assert "CS_ACCESS_TOKEN" not in source
    assert "fetch-depth: 0" not in source


def test_main_writes_the_ratchet_and_uploads() -> None:
    """Require main to publish the ratchet and CodeScene coverage."""
    steps = _steps("coverage-main.yml", "coverage-upload")
    coverage = next(
        step for step in steps if step.get("name") == "Test and Measure Coverage"
    )
    upload = next(
        step
        for step in steps
        if step.get("name") == "Upload coverage data to CodeScene"
    )
    assert coverage["with"]["with-ratchet"] == "true"
    assert (
        upload["uses"]
        == f"leynos/shared-actions/.github/actions/upload-codescene-coverage@{PIN}"
    )
    assert upload["with"]["mode"] == "upload"
