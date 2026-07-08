#!/usr/bin/env python3
"""Reconcile ESDiag collection definitions from support-diagnostics (ADR-0006).

ESDiag owns its per-product ``assets/<product>/sources.yml``; upstream
``support-diagnostics`` is a *reconciliation input*, not a runtime authority.
This script overlays the upstream definitions into ESDiag's files as a
FIELD-LEVEL merge:

* upstream owns:  ``versions`` (request paths + version gating), ``extension``,
  ``subdir``, ``retry`` — refreshed from upstream on every run
* ESDiag owns:    ``tags``, ``source_weight``, ``processing_weight``,
  ``streamable``, ``processable``, ``required``, ``dependencies``,
  ``collect_dependencies`` — always preserved (a blind copy would wipe them)

Upstream version ranges use the semver4j/NPM dialect (space-separated
clauses); they are normalized into native Rust ``semver`` form (comma-
separated clauses) at this boundary, so the runtime parses ranges with stock
``semver::VersionReq`` and needs no compatibility shim.

Deliberate divergences (sources ESDiag renames, adds, removes, or corrects)
are recorded in ``assets/<product>/sources-divergences.yml`` and are never
reverted by reconciliation.

Cadence (ADR-0006): run on EVERY application release (Elasticsearch, Kibana,
Logstash) AND every support-diagnostics release. See
``docs/source-reconciliation.md``.

Usage:
    scripts/reconcile_sources.py --support-diagnostics <checkout> \
        [--product elasticsearch|kibana|logstash] [--check]

``--check`` reports the diff without writing, for CI/review use.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

try:
    import yaml
except ImportError:  # pragma: no cover
    sys.exit("PyYAML is required: pip install pyyaml")

# Fields refreshed from upstream on every reconciliation.
UPSTREAM_FIELDS = ("versions", "extension", "subdir", "retry")
# ESDiag enrichments: never overwritten by the overlay.
ESDIAG_FIELDS = (
    "tags",
    "source_weight",
    "processing_weight",
    "streamable",
    "processable",
    "required",
    "dependencies",
    "collect_dependencies",
)

# Upstream file per product within a support-diagnostics checkout.
UPSTREAM_FILES = {
    "elasticsearch": "src/main/resources/elastic-rest.yml",
    "kibana": "src/main/resources/kibana-rest.yml",
    "logstash": "src/main/resources/logstash-rest.yml",
}
# OS-command definitions (self-managed syscalls etc.) live in diags.yml and
# overlay into the elasticsearch registry.
UPSTREAM_DIAGS = "src/main/resources/diags.yml"


def normalize_semver_range(expr: str) -> str:
    """Convert an NPM/semver4j dialect range into native Rust semver form.

    ``">= 5.0.0 < 7.0.0"`` becomes ``">= 5.0.0, < 7.0.0"``: clause boundaries
    (whitespace between a version and the next operator) become commas.
    """
    return re.sub(r"(\d)\s+(?=[<>=~^])", r"\1, ", expr.strip())


def normalize_versions(versions: dict) -> dict:
    return {normalize_semver_range(k): v for k, v in versions.items()}


def load_yaml(path: Path) -> dict:
    if not path.exists():
        return {}
    with path.open() as fh:
        return yaml.safe_load(fh) or {}


def load_upstream_registry(support_diagnostics: Path, product: str) -> dict:
    upstream = load_yaml(support_diagnostics / UPSTREAM_FILES[product])
    if product == "elasticsearch":
        upstream.update(load_yaml(support_diagnostics / UPSTREAM_DIAGS))
    return upstream


def overlay(esdiag: dict, upstream: dict, divergences: dict) -> tuple[dict, list[str]]:
    """Field-level merge of upstream into ESDiag's registry.

    Returns the merged registry and a human-readable change log.
    """
    changes: list[str] = []
    merged = {key: dict(value) for key, value in esdiag.items()}

    renames = divergences.get("renames", {})  # upstream key -> esdiag key
    removed = set(divergences.get("removed", []))  # upstream keys esdiag drops
    owned = set(divergences.get("esdiag_only", []))  # keys with no upstream

    for upstream_key, upstream_entry in (upstream or {}).items():
        if upstream_key in removed:
            continue
        key = renames.get(upstream_key, upstream_key)
        entry = merged.setdefault(key, {})
        is_new = not any(field in entry for field in UPSTREAM_FIELDS + ESDIAG_FIELDS)

        for field in UPSTREAM_FIELDS:
            if field not in upstream_entry:
                continue
            value = upstream_entry[field]
            if field == "versions":
                value = normalize_versions(value)
            if entry.get(field) != value:
                changes.append(f"{key}: refreshed `{field}` from upstream")
                entry[field] = value

        if is_new:
            changes.append(f"{key}: NEW upstream source added (review weights/tags)")

    # Report upstream keys that vanished (esdiag keeps its entry; removal is a
    # human decision recorded as a divergence).
    upstream_keys = {renames.get(k, k) for k in (upstream or {})}
    for key in sorted(set(merged) - upstream_keys - owned):
        changes.append(f"{key}: not present upstream (esdiag-only; record in divergences if intended)")

    return merged, changes


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--support-diagnostics", required=True, type=Path)
    parser.add_argument("--product", choices=sorted(UPSTREAM_FILES), action="append")
    parser.add_argument("--check", action="store_true", help="report changes without writing")
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parent.parent
    products = args.product or sorted(UPSTREAM_FILES)
    exit_code = 0

    for product in products:
        upstream_path = args.support_diagnostics / UPSTREAM_FILES[product]
        esdiag_path = repo_root / "assets" / product / "sources.yml"
        divergences_path = repo_root / "assets" / product / "sources-divergences.yml"

        upstream = load_upstream_registry(args.support_diagnostics, product)
        if not upstream:
            print(f"[{product}] no upstream file at {upstream_path}, skipping")
            continue
        esdiag = load_yaml(esdiag_path)
        divergences = load_yaml(divergences_path)

        merged, changes = overlay(esdiag, upstream, divergences)

        if not changes:
            print(f"[{product}] in sync with upstream")
            continue

        print(f"[{product}] {len(changes)} change(s):")
        for change in changes:
            print(f"  - {change}")

        if args.check:
            exit_code = 1
        else:
            with esdiag_path.open("w") as fh:
                yaml.safe_dump(merged, fh, sort_keys=True, default_flow_style=False)
            print(f"[{product}] wrote {esdiag_path}")
            print(f"[{product}] NOTE: safe_dump drops comments/ordering; review the diff before committing.")

    return exit_code


if __name__ == "__main__":
    sys.exit(main())
