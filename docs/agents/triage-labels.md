# Triage Labels

The skills speak in terms of five canonical triage roles. This file maps those roles to the actual label strings used in this repo's issue tracker (`elastic/esdiag`).

| Label in mattpocock/skills | Label in our tracker | Meaning                                  |
| -------------------------- | -------------------- | ---------------------------------------- |
| `needs-triage`             | `needs-triage`       | Maintainer needs to evaluate this issue  |
| `needs-info`               | `needs-info`         | Waiting on reporter for more information |
| `ready-for-agent`          | `ready-for-agent`    | Fully specified, ready for an AFK agent  |
| `ready-for-human`          | `ready-for-human`    | Requires human implementation            |
| `wontfix`                  | `Wontfix`            | Will not be actioned                     |

When a skill mentions a role (e.g. "apply the AFK-ready triage label"), use the corresponding label string from this table.

## Notes for this repo

- `wontfix` maps to the pre-existing **`Wontfix`** label on `elastic/esdiag` (note the capitalization — apply it exactly).
- The other four roles have **no existing equivalent** on `elastic/esdiag` and use their default strings. `/triage` will create them on first use (`gh label create <name> --repo elastic/esdiag`).
- `Question` ("Further information is requested") already exists and is semantically close to `needs-info`; we chose a dedicated `needs-info` label rather than overloading `Question`.

Edit the right-hand column to match whatever vocabulary you actually use.
