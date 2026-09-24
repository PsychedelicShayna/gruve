# Triage labels

The skills speak in terms of five canonical triage roles. This file maps those roles to the actual label strings used in this repo's issue tracker.

| Label in mattpocock/skills | Label in our tracker | Meaning                                  |
| -------------------------- | -------------------- | ---------------------------------------- |
| `needs-triage`             | `needs-triage`       | Maintainer needs to evaluate this issue  |
| `needs-info`               | `needs-info`         | Waiting on reporter for more information |
| `ready-for-agent`          | `ready-for-agent`    | Fully specified, ready for an AFK agent  |
| `ready-for-human`          | `ready-for-human`    | Requires human implementation            |
| `wontfix`                  | `wontfix`            | Will not be actioned                     |

When a skill mentions a role (e.g. "apply the AFK-ready triage label"), use the corresponding label string from this table.

Edit the right-hand column to match whatever vocabulary you actually use.

## House labels

Copied from `PsychedelicShayna/neopi`. These are an orthogonal layer, not aliases for the five roles above. An issue may carry both. Do not strip house labels to be Matt-only.

| Label | Meaning |
| --- | --- |
| `bug` | Something isn't working |
| `enhancement` | New feature or request |
| `effort: tiny` | Under 2 hours for a competent AI agent with a plan |
| `effort: small` | Roughly 2–6 hours for a competent AI agent with a plan |
| `effort: medium` | Roughly 1–2 days for a competent AI agent with a plan |
| `effort: large` | Roughly 3–7 days for a competent AI agent with a plan |
| `effort: very large` | Multi-week or cross-cutting architectural work |
| `priority: p0` | Blocks current work or handles an active incident |
| `priority: p1` | High value or prerequisite for several issues |
| `priority: p2` | Useful planned work |
| `priority: p3` | Backlog or opportunistic work |
| `entangled` | Alternative implementations of one idea; shipping any one supersedes the siblings |

Apply exactly one `effort:*` and exactly one `priority:*` when publishing a scoped issue. Apply `entangled` only for a true alternative cluster. Other GitHub defaults (`documentation`, `duplicate`, `question`, and the rest) stay available; use them only when they fit.
