
Please do a thorough review of our agents scaffold!

Where can we improve it to empower agents working on our aria-nbv masters thesis project?


Ran 24 commands, read 26 files
I have enough context for a thorough review. Here it is.

ARIA-NBV agent scaffold — review
What's working well
Role-split source order (.agents/references/source_order.md) plus the thin-root → progressive-disclosure dispatcher in AGENTS.md is clean: thesis docs own direction, glossary owns terms, proposal Typst owns advisor wording, seminar paper is historical evidence only, and .agents/memory/state/ owns current truth. The Conflict Rule and Capture Rule make this auditable, not aspirational.
Karpathy-style lane discipline in agent-behavior (state assumptions → inspect owner → narrowest edit → verify → capture in smallest surface) is the right backbone for advisor-facing thesis work where unbounded LLM "improvements" are a real risk.
Skill metadata (mode, not_when, handoff_to, evidence_required, applies_to, triggers, must_read, verification) is unusually rigorous — machine-routable and explicit about negative scope. Bigger projects copy half of this; you have the full set.
Agents-DB as TOML + make agents-db keeps backlog local, diffable, validation-checked, and wired to litkg through stable refs (repo:/bib:/arxiv:/s2:/litkg:). 17 skills, 52 May debriefs, validator + provenance — the scaffold is being used, not aspirational.
Hard contracts captured as references, not as skill prose: rollout_zarr_q_invalidity_contract.md, verification_matrix.md, python_conventions.md. This keeps SKILL.md bodies under the 150-line target.
External submodule (.agents/external/litkg-rs) pulls KG implementation out of the repo while keeping it agent-addressable via the semantic-scholar-litkg skill — clean separation of "tooling repo" from "thesis repo".
High-impact gaps (actionable)
1. The scaffold is invisible to Claude Code
.claude/ is empty. Zero references to "claude" anywhere. The whole agent stack is Codex (AGENTS.md, .codex/, .codex-plugin/) plus a tiny .gemini/. Concrete fixes:

CLAUDE.md symlink → AGENTS.md (or a 5-line redirect file). Without this, Claude Code does not auto-load any of the careful root routing you wrote.
.claude/settings.json with: SessionStart hook running make check-agent-memory (you already have this in .codex/hooks.example.json); PreCompact hook running make agents-db AGENTS_ARGS='validate'; Stop hook nudging "did this task warrant a debrief under .agents/memory/history/YYYY/MM/?"
.claude/commands/ for the highest-traffic verbs you already named: /debrief, /agents-db-validate, /check-memory, /diagnose, /plan-grill, /kg-claim-check, /context-refresh. Each is a thin file that reads the relevant skill + runs the verification command — much faster than Codex-style skill loading on every turn.
.claude/agents/ subagents for a few clear lanes: a Reviewer (wraps code-review-aria-nbv), a Diagnoser (wraps diagnose-aria), a Plan-griller, a Debrief-writer. These get cold-context isolation, which matters when the main session is doing thesis writing and you want a parallel review without polluting context.
2. SKILL frontmatter ≠ Claude Skills
Your skill format is Codex/agents-style; Claude Code "skills" are different (auto-triggered by description matching). If you want them to fire in Claude sessions:

Either add a top-level description field (Claude reads only that to decide activation), or
Generate a sibling .claude/skills/<name>/SKILL.md with a Claude-compatible frontmatter from your existing files. A small make claude-skills target could produce them deterministically from .agents/skills/*/SKILL.md.
Without this, the only way Claude reaches your skill bodies is when you (or agent-behavior) explicitly say "read X."

3. No automation around the debrief/memory contract
You require a debrief for non-trivial work and canonical_updates_needed in every native debrief — but it's all on the human/agent's discipline.

Add make new-debrief TITLE="..." that scaffolds the YAML frontmatter (with today's date already filled, since date: drift is a known failure mode in long sessions).
Add a Stop/SubagentStop hook that runs a quick check: "session touched > N files but no new file under .agents/memory/history/YYYY/MM/ — warn." The validator already exists; wire it to the lifecycle.
Move the "convert relative dates to absolute" rule (currently only in your Claude memory system prompt) into agent_memory_templates.md as a hard rule — debriefs will outlive the session and "Thursday" rots fast.
4. AGENTS.md root is dense for a "thin root"
86 lines, 11 routing bullets, 7 non-negotiables, 6 verification kinds, 6 commands. It crosses the line where new agents stop reading by line 40.

Compress Routing into a 3-column table (signal | lane | handoff) — easier to scan than prose bullets.
Move "Commands" to .agents/references/operator_quick_reference.md (already exists). Root only needs make context, make agents-db, make check-agent-memory as the three doors.
Non-Negotiables 6/7 ("invalidity is a hard mask", "GT actor-visible") are research contracts, not scaffold rules — they belong in .agents/memory/state/DECISIONS.md (already partially there). Root should keep only invariants that govern agent behavior, not thesis claims.
5. Worktree-aware memory is missing
You're literally running this review inside .claude/worktrees/pedantic-buck-d6f491/. Worktrees fork .agents/memory/state/ and .agents/*.toml — but nothing in the scaffold says how to merge debriefs, backlog edits, or memory-state changes back to main. With multiple parallel agent sessions on a thesis, this will silently diverge.

Document worktree policy in human_owner_intent.md or a new .agents/references/worktree_policy.md: e.g., "debriefs always commit on the worktree branch; canonical state edits (.agents/memory/state/*.md) require a rebase + make check-agent-memory before merging."
Optional: a make worktree-debrief-promote that cherry-picks just history files into main without dragging unrelated WIP.
6. KG / litkg is "probationary" but used in many skills' verification
Six skills name make kg-* commands as verification. If litkg is degraded or the user is offline, every one of those verifications stalls.

Add a make kg-status shortcut that returns a 0/1 health snapshot, and have the skills' verification prefer the smallest local check + KG only when authority/freshness matters. The aria-litkg-memory skill already says "fall back to aria-nbv-context plus targeted file reads" — propagate that fallback into skills that today only list make kg-*.
7. Discoverability of .agents/skills/aria-nbv-context/scripts/
You have eight context scripts (nbv_qmd_outline.py, nbv_typst_includes.py, nbv_literature_index.sh, nbv_get_context.sh modules|contracts|match, …) and they're surfaced only in the body of aria-nbv-context/SKILL.md. That skill body is gated behind "agent reads SKILL.md when it decides this lane fits."

List the script signatures in operator_quick_reference.md under a "Local discovery" section so they're reachable from the root quick-ref path too.
Add --help output to each (some already have it, the shell ones may not) so an agent that finds the file can self-discover args without re-reading the skill.
8. python-docstrings skill quality is uneven across packages
You have a strict skill but no automated lint for the contract it asserts (e.g., "tensor shapes documented when not obvious", attribute docstrings vs. Field(description=...)). Consider:

A ruff rule selection (e.g., D101, D102, D205, D417) limited to public surfaces (aria_nbv/aria_nbv/**/*.py excluding tests), wired into make lint-docstrings.
Or a tiny custom check that flags Field(default=<callable>) and Field(..., description=…) on BaseConfig subclasses — both are explicitly listed as anti-patterns in python_conventions.md and are easy to detect.
9. No "task preflight" template
agent-behavior lists principles but not a concrete checklist. New agents reinvent the preflight every session.

Add a tiny .agents/skills/agent-behavior/references/preflight.md:
not done
Read root AGENTS.md Routing
not done
Identify lane + named handoff
not done
List touched surfaces + nearest AGENTS.md
not done
State assumption + success criteria in one sentence each
not done
Name the verification command from verification_matrix.md
Reference it from agent-behavior and root AGENTS.md. This is the same content you have, but in a form an agent can actually paste into the session as a checklist.
10. Resolved/done items aren't surfacing as positive signal
resolved.toml is 802 lines — gold for "what worked" — but no skill or reference points there. Plan-grill / diagnose-aria could cite resolved precedent before re-deriving an approach.

Add a make agents-db AGENTS_ARGS='search <term>' (if not already there) and reference it from plan-grill and diagnose-aria as the first step ("has this been tried before?").
Smaller items
.agents/skills/diagnose-aria/ has only SKILL.md (no references/, no agents/). Compared to peers it's missing the agents/openai.yaml interface stub — minor, but inconsistent.
Skill style guide says "default skill bodies should stay under ~150 lines" — code-review/SKILL.md is 142, lrz-ai-systems/SKILL.md body is dense; both are fine, but code-review could push some of the PR/working-tree command lists into a references/ file to free room for the things that actually require an LLM (severity rubric, frame-consistency targets).
default_profile.yaml at repo root is empty. Either populate it or remove — empty files invite agent guessing.
.codex/config.example.toml and .codex/hooks.example.json templates exist but .gemini/settings.json is real, not example. Consider matching: either commit a real .codex/config.toml (with operator-local stuff gitignored) or rename the gemini one to .example.json for parity. Right now the asymmetry suggests one tool is "supported" and the other is "documented."
AGENTS_INTERNAL_DB.md says "Priority Pillar 4: Agentic Lifecycle — Port PR and issue lifecycle workflows" — this maps directly to my point 1 (Claude-native subagents/commands) and refactor-002. It's already on your list; the gap is that nothing has been ported yet.
Summary recommendation
Three highest-leverage things, in order:

Make the scaffold first-class for Claude: CLAUDE.md redirect + .claude/settings.json (SessionStart/PreCompact/Stop hooks reusing your existing make targets) + 4-6 slash commands wrapping your most-used verbs. ~1 evening of work, immediate compounding return on every Claude session.
Automate the debrief contract with a make new-debrief scaffolder and a Stop-hook reminder. The discipline is documented; without automation it relies on memory.
Compress root AGENTS.md to a routing table + Capture Rule + three commands; push the rest down into the references that already exist. The thin-root claim is in DECISIONS.md but the root file is no longer thin.
Everything else (worktree policy, KG fallbacks, docstring lint, preflight checklist, resolved-precedent search) is lower-cost-but-still-valuable polish.

another agent has just added some items to the agents-db please consollidate those  with your findings!


Ran 4 commands
The other agent added refactor-016 "Distill agent scaffold ownership and DB record style" (priority high, linked to issues 012/023/025), and rewrote issue-012, issue-023, issue-025, and todo-056 to be narrower. Significant overlap with my review — here's the consolidation.

My findings now subsumed by refactor-016
The other agent's record covers these directly; no new DB entries needed:

My item	Captured in refactor-016 as
Root AGENTS.md is dense; move research non-negotiables out (V0/V1, invalidity, Gym/SB3)	"Trim root AGENTS.md to repo-wide operational invariants; leave V0/V1, invalidity, Gym/SB3 gates in domain skills/thesis sources"
Move Commands block to operator_quick_reference.md	"Shorten root, docs, code-review, simplification hot paths; move long content into optional references"
Skill metadata inconsistency across 17 skills	"skill_style_guide define universal metadata floor + optional broad-skill routing extras; remove unsupported/misleading mode values"
DECISIONS.md (185 lines) repeats roadmap narrative; PROJECT_STATE.md likewise	"Condense DECISIONS.md to durable deltas; PROJECT_STATE.md to short snapshot"
Verbose TOML records / essay-like context	"Backlog records and schema no longer encourage essay-like context or pseudo-precise LOC triplets for ordinary work"
Generic instruction-capture policy duplicated across AGENTS.md / source_order.md / human_owner_intent.md	"Exactly one source owns instruction-capture policy; human_owner_intent.md contains only Jan-specific preferences plus a pointer"
litkg_quick_reference.md carries full schema + trust ladder	"Quick reference covers usage/commands/fallback/claim checks; full trust/schema lives in one deeper owner" (also reflected in todo-056 narrowing)
These need no separate records; the cleanup is owned.

My findings not covered — proposed DB additions
These are orthogonal to refactor-016 (which is about distilling existing surfaces, not adding tooling). Suggest two new records and one note.

Proposed issue-NNN — "Claude Code is not a first-class agent surface"
problem: Zero claude references in repo; .claude/ empty; CLAUDE.md absent; no .claude/settings.json hooks; no .claude/commands/; no .claude/agents/. Claude sessions don't auto-load AGENTS.md, your SKILL.md format doesn't trigger Claude Skills (which match on description), and the well-tested .codex/hooks.example.json (make check-agent-memory on SessionStart, make agents-db AGENTS_ARGS='validate' on PreCompact) is unavailable.
why-now: We already use Claude alongside Codex/Gemini; the scaffold's careful routing only applies when the agent finds it. Worktrees under .claude/worktrees/ are evidence this is in active use.
acceptance:

CLAUDE.md redirects to AGENTS.md (symlink or 5-line pointer)
.claude/settings.json mirrors .codex/hooks.example.json lifecycle hooks
.claude/commands/ covers /debrief, /check-memory, /agents-db-validate, /diagnose, /plan-grill, /kg-claim-check
.claude/agents/ declares Reviewer, Diagnoser, Plan-griller, Debrief-writer wrapping existing skills
Optional: make claude-skills regenerates Claude-format skill stubs from .agents/skills/*/SKILL.md
verification: test -L CLAUDE.md || test -f CLAUDE.md; jq . .claude/settings.json; smoke a slash command.
relation: extends refactor-002 ("Port PR and issue lifecycle skills"); does not duplicate it (that's GitHub-PR workflow; this is per-vendor scaffold parity).
Proposed refactor-NNN — "Automate the debrief / canonical-update contract"
problem: agent_memory_templates.md mandates frontmatter and canonical_updates_needed, but enforcement is human discipline. validate_agent_memory.py only checks files that exist; nothing nudges agents to write one. With 52 May debriefs, the cost of a missed one is real.
why-now: Pairs with refactor-016's record-style simplification — automation is what makes "compact records" sustainable.
acceptance:

make new-debrief TITLE="..." scaffolds today's .agents/memory/history/YYYY/MM/<slug>.md with frontmatter pre-filled (absolute date, not relative)
Stop / SubagentStop hook (Codex + Claude) flags "session touched ≥ N tracked files but no new file under .agents/memory/history/YYYY/MM/"
agent_memory_templates.md adds an explicit "convert relative dates to absolute on write" rule
verification: make new-debrief TITLE='smoke' produces a valid frontmatter file that passes make check-agent-memory.
Proposed todo-NNN (small) — "Wire scaffold polish that doesn't fit refactor-016"
Bundle these so they don't sprawl:

Worktree-merge policy doc at .agents/references/worktree_policy.md (you're literally running in .claude/worktrees/pedantic-buck-d6f491/; nothing says how .agents/memory/state/ and .agents/*.toml reconcile across parallel sessions).
KG-degraded fallback: skills whose verification only lists make kg-* should also name a local check; make kg-status returns 0/1 for fast triage.
Surface aria-nbv-context/scripts/ (8 scripts) in operator_quick_reference.md so they're reachable without first loading the skill body.
plan-grill and diagnose-aria add "search resolved.toml first" as step 1 — 802 lines of unused precedent.
Drop empty default_profile.yaml; rename .gemini/settings.json to .example.json for symmetry with .codex/*.example.* (or commit a real .codex/config.toml with operator-local fields gitignored).
diagnose-aria/ skill missing the agents/openai.yaml stub all peers carry.
Implementation-notes nudge for refactor-016 itself
Two adjacents you may want folded in if you keep refactor-016 as the single distillation owner:

Docstring contract enforcement (point 8 in my original review): add a make lint-docstrings (ruff D-rules limited to aria_nbv/aria_nbv/** public surfaces + a check for Field(default=<callable>) and Field(..., description=…) on BaseConfig subclasses, both already named anti-patterns in python_conventions.md). Fits naturally under the "compact contracts, automatable enforcement" theme of refactor-016.
Task preflight artifact (agent-behavior/references/preflight.md) — paste-able checklist version of what agent-behavior/SKILL.md already says in prose. Same theme: turn discipline into a callable surface.
If you'd rather keep refactor-016 strictly distillation, both can ride on the new debrief-automation refactor instead.

Quick action
Want me to draft the three TOML records as a patch for make agents-db AGENTS_ARGS='validate' to consume, or just leave them as text for you to file?

please proceed then implement changes!

Proceeding. First I'll align with the agents-db schema and the validator, then add the three records, then implement the Claude scaffold.


Ran 6 commands
Plan: add 3 TOML records → validate → build the Claude scaffold (CLAUDE.md + .claude/settings.json + commands + agents) → wire make new-debrief → write a debrief → run check-agent-memory.


Read 3 files, edited 4 files, ran 4 commands
Records validated. Now building the Claude scaffold.


Created 2 files
Now the slash commands.


Created 7 files
Now subagents.


Created 4 files
Now the make new-debrief script + Makefile target + Stop hook nudge.


Ran 2 commands, read a file, created a file, edited 2 files
Now the Stop-hook nudge script.


Created a file, ran a command
Now wire the same nudge into .codex/hooks.example.json and add the absolute-date rule to agent_memory_templates.md.


Edited 3 files, read a file
Smoke-test the new debrief target and validate everything.

Ran
Scaffold today's debrief
The worktree has no aria_nbv/.venv/. Running the script directly (stdlib only).


Ran 2 commands
Both validators pass. Now writing the debrief body.


Created 2 files, read a file, ran a command
All landed. Both validators pass; the Stop-hook nudge correctly suppresses because today's debrief exists.

What landed
Agents-DB records (validate clean):

issue-029 "Claude Code is not a first-class agent surface" (high, maintenance)
refactor-017 "Automate debrief and canonical-update contract" (medium, links issue-012 + issue-029)
todo-061 "Wire scaffold polish that does not fit refactor-016 or refactor-017" (low, links issue-012 + issue-029)
Claude scaffold (point 1 of issue-029):

CLAUDE.md → routes Claude into AGENTS.md and lists Claude-specific surfaces
.claude/settings.json mirrors .codex/hooks.example.json (SessionStart → make check-agent-memory, PreCompact → agents-DB validate, Stop → debrief nudge)
.claude/commands/: /check-memory, /agents-db, /debrief, /diagnose, /plan-grill, /kg-claim-check, /context-refresh
.claude/agents/: aria-reviewer, aria-diagnoser, aria-plan-grill, aria-debrief-writer
Debrief automation (refactor-017, partial — the user-facing pieces):

scripts/new_debrief.py (stdlib only) + make new-debrief TITLE='...'
scripts/debrief_nudge.sh Stop hook for both .claude/settings.json and .codex/hooks.example.json
agent_memory_templates.md now mandates absolute ISO dates explicitly
Today's debrief at .agents/memory/history/2026/05/2026-05-07_claude_scaffold_bootstrap_and_debrief_automation.md
What I did not do
Touch refactor-016's scope (root AGENTS.md distillation, DECISIONS.md/PROJECT_STATE.md compaction, skill-metadata floor, human_owner_intent.md slimming, litkg_quick_reference.md quick-use-only). That's the other agent's owned cleanup.
Touch todo-061's items inline (worktree policy, kg-status, surfacing aria-nbv-context scripts in operator quick-ref, default_profile.yaml removal, .codex/.gemini symmetry, diagnose-aria/agents/ stub). Filed for later.
Convert your existing .agents/skills/*/SKILL.md to Claude-Skill activation format. Claude Code activates skills by description matching; your current frontmatter is Codex-shaped. A make claude-skills generator was named in issue-029 but isn't implemented yet — wanted to keep this commit reviewable.
Pre-existing gap noticed
make new-debrief itself fails in this worktree because Makefile requires aria_nbv/.venv/bin/python which is not provisioned per-worktree. The bare python3 scripts/new_debrief.py ... works (stdlib only). This is a worktree-venv gap, not a scaffold regression — captured in todo-061 under "worktree policy".

Files touched
M  .agents/issues.toml
M  .agents/refactors.toml
M  .agents/todos.toml
M  .agents/references/agent_memory_templates.md
M  .codex/hooks.example.json
M  Makefile
?? .agents/memory/history/2026/05/2026-05-07_claude_scaffold_bootstrap_and_debrief_automation.md
?? .claude/
?? CLAUDE.md
?? scripts/debrief_nudge.sh
?? scripts/new_debrief.py
please proceed as suggested!

Proceeding with todo-061 items + the Claude-skills bridge. Starting with the trivial pieces, then the larger ones.


Ran 2 commands
Empty and unreferenced — safe to delete. Also adding the diagnose-aria/agents/openai.yaml stub for parity.


Ran a command, read a file, created a file
Now the worktree policy doc.

Created
worktree_policy.md
+53
-0
Now make kg-status and a small KG health probe.


Ran 2 commands, read a file, edited 2 files, created a file
Probe works (correctly reports the submodule isn't initialized in this worktree). Now adding KG fallbacks to the two skills whose verification is KG-only.


Edited 3 files
Now agents_db.py search, then plan-grill / diagnose-aria pointing at it.


Read a file, edited a file, ran 2 commands
Good. Now wire it into plan-grill and diagnose-aria skills.


Edited 3 files
Now surface the aria-nbv-context scripts in operator_quick_reference.md.


Read a file, edited a file
Now the Claude-skills bridge — symlinks plus a make target so they stay in sync.


Created a file, ran a command, edited 2 files
Update CLAUDE.md to mention the sync target and the claude-skills symlinks; also gitignore stale claude-skill mismatches won't be an issue but let me verify.

Edited
CLAUDE.md
+7
-2
Final validation pass + check make check-agent-memory is happy with the new files.


Ran 2 commands
Update today's debrief to reflect the additional work just landed.


Edited 2 files, ran a command
All clean. Summary below.

What landed this round (todo-061 + Claude-skills bridge)
Worktree policy (.agents/references/worktree_policy.md):

Memory-surface merge-risk table, per-session workflow, ID-collision avoidance, conflict policy.
Documents the worktree-venv gap (Makefile expects aria_nbv/.venv per worktree; falls back to python3 scripts/...).
KG fallback (make kg-status + scripts/kg/status.sh):

Fast 0/1 probe — checks for .configs/litkg.toml, the litkg-rs submodule, and cargo on PATH. Exits 1 with a one-line reason; never blocks.
Wired into aria-litkg-memory/SKILL.md and verification_matrix.md as the first check before heavier make kg-* commands.
Resolved-precedent search (python3 scripts/agents_db.py search <term>):

New subcommand, covers active and resolved.toml; case-insensitive across id, title, description, labels, context, implementation_notes, acceptance.
--scope active|resolved|all to narrow.
plan-grill and diagnose-aria skills now call this as step 1 ("has this been tried?").
agents-db skill's commands list updated.
Operator quick-reference (operator_quick_reference.md):

New "Local Discovery Scripts" table surfacing the eight aria-nbv-context/scripts/ helpers (nbv_get_context, nbv_qmd_outline, nbv_typst_includes, nbv_literature_*, nbv_context_index).
Skill polish:

Deleted empty default_profile.yaml.
Added .agents/skills/diagnose-aria/agents/openai.yaml stub for parity with peer skills.
Claude-skills bridge (make claude-skills + scripts/sync_claude_skills.sh):

Idempotent symlink sync: .claude/skills/<name> → ../../.agents/skills/<name> for all 17 skills.
Removes stale symlinks; refuses to clobber non-symlinks.
CLAUDE.md updated to mention the sync target.
What I deliberately did not do
.codex ↔ .gemini asymmetry: .gemini/settings.json is committed real config; .codex/config.example.toml is a template; the operator's choice for Codex is gitignored. Resolving this means either committing a real .codex/config.toml (with secrets gitignored) or renaming .gemini/settings.json to .example.json — both have user-facing tradeoffs. Left for human decision; called out in todo-061.
refactor-016's scope (root AGENTS.md distillation, DECISIONS.md/PROJECT_STATE.md compaction, skill-metadata floor, human_owner_intent.md slimming, litkg_quick_reference.md quick-use-only). Owned by the other agent.
Current diff in this worktree
M  .agents/issues.toml                              (+ issue-029)
M  .agents/refactors.toml                           (+ refactor-017)
M  .agents/todos.toml                               (+ todo-061)
M  .agents/references/agent_memory_templates.md     (absolute-date rule)
M  .agents/references/operator_quick_reference.md   (discovery scripts table)
M  .agents/references/verification_matrix.md        (kg-status fallback)
M  .agents/skills/agents-db/SKILL.md                (search subcommand)
M  .agents/skills/aria-litkg-memory/SKILL.md        (kg-status fallback)
M  .agents/skills/diagnose-aria/SKILL.md            (resolved-search step 1)
M  .agents/skills/plan-grill/SKILL.md               (resolved-search grounding)
M  .codex/hooks.example.json                        (Stop nudge)
M  Makefile                                         (new-debrief, kg-status, claude-skills targets)
M  scripts/agents_db.py                             (search subcommand)
D  default_profile.yaml
?? .agents/memory/history/2026/05/2026-05-07_claude_scaffold_bootstrap_and_debrief_automation.md
?? .agents/references/worktree_policy.md
?? .agents/skills/diagnose-aria/agents/openai.yaml
?? .claude/                                         (settings.json, commands/, agents/, skills/ symlinks)
?? CLAUDE.md
?? scripts/debrief_nudge.sh
?? scripts/kg/status.sh
?? scripts/new_debrief.py
?? scripts/sync_claude_skills.sh
Validators (agents_db validate, validate_agent_memory) both pass.



Select any text to leave a comment for Claude
Plan — Implement semantic kg-search in three commits (todo-066, todo-070, todo-067)
Context
User direction (2026-05-12): proceed with the recommended sequence from the
prior turn. Important state was just persisted to DECISIONS.md,
PROJECT_STATE.md, and GOTCHAS.md so this plan can be picked up
cold after a context compaction.

Five kg-*-related todos exist against issue-025:

todo	Scope	Dependency
todo-066	Phase A: BM25 + Porter stemming + synonyms + Levenshtein fuzzy fallback. Lexical floor.	None — independent of Neo4j.
todo-070	Phase B prerequisite: ingest Paper/DocSection/ProjectMemory nodes into the live Neo4j so the existing vector index kg_embedding_index_2560 covers them.	None — orthogonal to Phase A.
todo-067	Phase B: Neo4j vector-index query consumer in run_kg_find; ollama-embedding query call; hybrid blend with BM25; search_mode surfacing.	Depends on todo-070 for full literature coverage.
todo-062	Paper-node provenance — paper:* nodes lose ParsedPaper.provenance on ingest.	Independent.
todo-063	Lean ContextPack default — 242 KB → ≤50 KB by hiding bulky/legacy fields behind --full.	Independent.
The three semantic-search todos (066/067/070) get executed in this plan.
062/063 stay independent and can land in parallel; they're documented
elsewhere.

Discovery captured
The vector index already exists (created idempotently by
enrich_embeddings.py:502). The 1936 embedded nodes are all
CodeGraphContext code symbols; zero paper / docsection / memory nodes
have embeddings yet. Manual Cypher smoke proved the query path works
(returns nearest neighbors at cosine ~0.74). The gap that breaks the
"Hestia for hierarchical viewpoint" use case is purely about ingesting
the non-code content into Neo4j — todo-070.

Decisions locked
From this session (now persisted in DECISIONS.md):

Neo4j vector index is the canonical home for semantic search.
kg_embedding_index_2560 over KGEmbeddingNode.kg_embedding,
HNSW + cosine, dim 2560.
BM25 (k1=1.5, b=0.75) replaces naive term-frequency as the
lexical scorer.
Compact-by-default kg-* output stays; KG_VERBOSE=1 /
KG_FORMAT=json to escape.
Graphiti / MemPalace stay deferred (todo-068 / todo-069).
The agent-facing kg verb surface is kg-search, kg-route,
kg-claim-check, kg-status, kg-capabilities. Do not
reintroduce kg-brief, kg-query, kg-related,
kg-ingest-docs-smoke.
Authoritative paths + constants
Submodule root: .agents/external/litkg-rs/
Bolt: bolt://127.0.0.1:7687, HTTP: http://127.0.0.1:7474
Neo4j auth: neo4j:litkglocal (per .env.example)
Ollama: http://127.0.0.1:11434, model qwen3-embedding:4b,
dim 2560, chat model gemma4:26b
Vector index name: kg_embedding_index_2560
Label marker: KGEmbeddingNode + property kg_embedding
Search verdict labels: canonical (≥1.5) and active (≥1.2)
per ranking.rs:46-54
Existing compact filters: scripts/kg/compact_{route,search,claim_check}.jq
Stop-hook auto-refresh: scripts/kg/auto_refresh.sh
(with optional KG_NEO4J_AUTO_UP=1)
Commit 1 — todo-066 Phase A: BM25 + Porter stemming + synonyms + Levenshtein fuzzy
Why first: independent of Neo4j; fixes the user's typo case
(hierachical viewpoint) directly via BM25's IDF + fuzzy fallback;
shippable in 1–2 days; survives Neo4j-down.

Files
File	Change
.agents/external/litkg-rs/crates/litkg-core/Cargo.toml	Add rust-stemmers = "1.2" (Porter stemmer, ≈4 KB). Add strsim = "0.11" for Levenshtein (or inline ~40 LOC DP impl).
crates/litkg-core/src/ranking.rs	Extend tokenizer with stem(token) -> String; apply to both index build and query tokens. Implement score_bm25 (~80 LOC inline). Compute IDF table during index build; cache avgdl. Add Levenshtein fuzzy fallback that only fires on zero-exact-hits for tokens length ≥ 5. Apply synonyms from config.
crates/litkg-core/src/config.rs	Deserialize [synonyms] table from .configs/litkg.toml. Add optional [context_pack].bm25_k1, bm25_b knobs (defaults 1.5 / 0.75).
crates/litkg-core/src/inspect.rs:1297	Replace score_lexical: score as f32 with the BM25-derived value.
.configs/litkg.toml	Add [synonyms] table with the seeded ARIA-NBV terms (nbv, viewpoint, hierarchical, rri, obb, semi-dense; see todo-066 implementation_notes for the exact entries).
scripts/kg/compact_search.jq	Already handles a query_fixups field at top level (added in 4174cd6). Confirm it still renders correctly when Rust emits the new field.
crates/litkg-cli/tests/inspect_cli.rs	Add scenario tests: (a) stemming collapses viewpoint/viewpoints; (b) BM25 reorders so a paper with rarer terms outranks one with high-frequency common terms; (c) synonym expansion finds Hestia for "NBV view selection"; (d) fuzzy fallback corrects "hierachical" and surfaces query_fixups.
Acceptance
cd .agents/external/litkg-rs && cargo fmt --all --check && cargo test
cd /home/jd/repos/ARIA-NBV

# User's original typo case.
make kg-search KG_QUERY='hierachical viewpoint' KG_FORMAT=json \
  | jq '{fixups: .query_fixups, top: [.results[0:5] // .[0:5] | .[].title]}'
# expect query_fixups contains "hierachical → hierarchical" and
# at least one Hestia paper section in top 5.

# Stemming.
diff <(make kg-search KG_QUERY='viewpoint' KG_LIMIT=5 | grep -oE 'paper:[a-z0-9-]+') \
     <(make kg-search KG_QUERY='viewpoints' KG_LIMIT=5 | grep -oE 'paper:[a-z0-9-]+')
# expect identical paper IDs (stemming works)

# Synonym expansion.
make kg-search KG_QUERY='NBV view selection' KG_LIMIT=5 \
  | grep -E 'hestia|gen-nbv|vin-nbv'
# expect at least 2 NBV papers in top 5

# BM25 regression on a stable query.
make kg-search KG_QUERY='RRI' KG_LIMIT=5
# expect top hits remain aria_nbv.rri_metrics.* code symbols
Risk + rollback
BM25 reordering may surprise. Mitigation: regression-test on RRI,
oracle, counterfactual queries; flag ordering changes >25 % for
review before merge.
Synonym over-fire. Mitigation: cap at one expansion-hit per query
token; keep the synonym list short.
Stemmer over-collapse. Mitigation: Porter is well-tested; unit
tests on viewing/viewer (should NOT collapse to view).
Rollback: revert ranking.rs + the Cargo deps; index format
unchanged.
Commit 2 — todo-070 Phase B prerequisite: ingest Paper/DocSection/ProjectMemory into Neo4j
Why second: independent of Phase A; unblocks the literature side of
Phase B. Without this, todo-067 only surfaces code symbols.

Route choice
Route (a) — Neo4j-side ingest (recommended). Reuse the existing
crates/litkg-neo4j/src/lib.rs Neo4jSink that already writes papers as
nodes to a JSONL bundle; add a parallel function that writes them to
the live Neo4j via Bolt MERGE statements. Then re-run
enrich_embeddings.py so those new nodes get KGEmbeddingNode label +
kg_embedding property and become queryable through the vector index.

Route (b) — JSONL sidecar (fallback). If route (a) blocks on
Cypher/Bolt complexity, write embeddings.jsonl alongside
nodes.jsonl; have run_kg_find read the sidecar and do cosine in
Rust. Documented in todo-070; skip unless route (a) hits a wall.

Files (route a)
File	Change
crates/litkg-neo4j/src/lib.rs	Add pub fn sync_to_runtime(client: &Neo4jClient, papers: &[ParsedPaper]) -> Result<()>. Mirror the existing Neo4jSink::export (lines 37–186) but use Bolt MERGE statements instead of JSONL writes. Same node types: Paper, PaperSection (DocSection), Author, FieldOfStudy, ExternalId, Citation. Same edges. Idempotent via MERGE on node id. Also handle load_project_memory(config, papers)? so ProjectMemory + MemorySurface nodes get into the runtime.
crates/litkg-cli/src/main.rs	Add a kg sync-runtime subcommand wired to sync_to_runtime.
Makefile	Add kg-load-bundle: target that calls cargo run -p litkg-cli -- kg sync-runtime --config $(LITKG_CONFIG).
Makefile	Wire kg-refresh-lit to call kg-load-bundle after kg-export-neo4j, so the live DB stays in sync.
scripts/kg/enrich_embeddings.py	No change needed — the script discovers KGEmbeddingNode candidates from Neo4j via Cypher (line ~530) and embeds anything that lacks kg_embedding. As long as papers/docsections/memory nodes carry KGEmbeddingNode label after sync, the existing embedder will cover them on the next make kg-enrich.
crates/litkg-neo4j/tests	Add a test that sync_to_runtime produces the same node count as the JSONL export and that nodes are idempotent under re-run.
Acceptance
make kg-refresh-lit                              # writes JSONL + loads into Neo4j
make kg-enrich                                   # embeds the new nodes

# Verify paper coverage in Neo4j.
curl -sSf -u neo4j:litkglocal -H 'Content-Type: application/json' \
  -X POST 'http://127.0.0.1:7474/db/neo4j/tx/commit' \
  -d '{"statements":[{"statement":"MATCH (n:KGEmbeddingNode) WITH labels(n) AS lbls, count(n) AS cnt RETURN lbls, cnt ORDER BY cnt DESC"}]}' \
  | jq '.results[].data'
# expect rows for Paper, PaperSection (or DocSection), ProjectMemory,
# alongside the existing Function/Module/Class/File rows.

# Manual smoke of vector search.
qvec=$(curl -sSf http://127.0.0.1:11434/api/embeddings -H 'Content-Type: application/json' \
  -d '{"model":"qwen3-embedding:4b","prompt":"hierarchical NBV target proposal pose realization"}' | jq -c '.embedding')
curl -sSf -u neo4j:litkglocal -H 'Content-Type: application/json' \
  -X POST 'http://127.0.0.1:7474/db/neo4j/tx/commit' \
  -d "{\"statements\":[{\"statement\":\"CALL db.index.vector.queryNodes('kg_embedding_index_2560', 5, \$qvec) YIELD node, score RETURN coalesce(node.paper_id, node.id) AS pid, node.title AS title, score, labels(node) AS labels\", \"parameters\": {\"qvec\": $qvec}}]}" \
  | jq '.results[].data[].row'
# expect Hestia or Hestia paper sections in the top 5.
Risk + rollback
Bolt MERGE on 6500+ nodes may be slow first time. Mitigation: batch
statements, use UNWIND for bulk inserts.
ProjectMemory nodes pull from .agents/memory/ — ensure their
content is up-to-date before sync (call make check-agent-memory
first).
Rollback: DROP the new node labels in Neo4j or just stop calling
kg-load-bundle. The JSONL export remains unchanged.
Commit 3 — todo-067 Phase B: Rust query consumer in run_kg_find
Why last: depends on todo-070 for useful literature coverage.
Once todo-070 lands, this turns hybrid semantic search on.

Files
File	Change
crates/litkg-cli/src/main.rs:1160 run_kg_find	Probe bolt://127.0.0.1:7687. If down → search_mode = "lexical_only", mode_reason = "neo4j_unreachable", skip to Phase A path. If up → compute query embedding once via the existing ollama HTTP client; run CALL db.index.vector.queryNodes('kg_embedding_index_2560', $k, $qvec); blend α·bm25 + (1-α)·cosine (default α=0.4); merge and dedup by node id; emit search_mode = "hybrid" and the new mode_reason field.
crates/litkg-cli/src/main.rs KgFindCommand	Add --lexical-only flag for explicit opt-out. Add --alpha <f32> for tuning.
crates/litkg-neo4j/src/lib.rs	Add pub fn query_vector_index(client, index_name: &str, k: usize, qvec: &[f32]) -> Result<Vec<(String, f32)>>. Reuse the existing Bolt connection pool.
crates/litkg-core/src/ranking.rs	Add cosine_similarity(a: &[f32], b: &[f32]) -> f32 (inline f32 SIMD-friendly). Add combine_lexical_and_cosine(bm25: f32, cos: f32, alpha: f32) -> f32.
crates/litkg-cli/src/main.rs ollama call	Either shell out to python3 scripts/kg/ollama_http.py embed --text "<q>" (simplest first iteration) or port a thin Rust client using reqwest. Shell-out adds ~30 ms latency per query; acceptable for v1.
scripts/kg/compact_search.jq	Already accepts both bare-array and {results, search_mode, mode_reason, query_fixups} wrapper shapes (since 4174cd6). Confirm the new wrapper renders with the real search_mode + mode_reason values.
crates/litkg-cli/tests/inspect_cli.rs	Add scenario tests: (a) hybrid mode active and Hestia surfaces for the semantic query; (b) lexical fallback when Neo4j is down; (c) lexical fallback when ollama is down; (d) --alpha parameter tunes the blend.
Acceptance
cd .agents/external/litkg-rs && cargo fmt --all --check && cargo test
cd /home/jd/repos/ARIA-NBV

# Pure semantic (zero token overlap).
make kg-up
make kg-refresh-light
make kg-search KG_QUERY='how do hierarchical NBV methods decompose target proposal from pose realization' KG_FORMAT=json \
  | jq '{mode: .search_mode, top: [.results[0:3] | .[].title]}'
# expect mode=hybrid, top includes Hestia or related.

# Hybrid mode hint.
make kg-search KG_QUERY='RRI' KG_FORMAT=json | jq -r '.search_mode'
# expect "hybrid"

# Fallback.
make kg-down
make kg-search KG_QUERY='RRI' KG_FORMAT=json | jq -r '{mode: .search_mode, reason: .mode_reason}'
# expect mode=lexical_only, reason=neo4j_unreachable
make kg-up  # restore
Risk + rollback
Query latency adds ~80 ms (ollama embedding). Mitigation: cache last
query embedding in-process; HNSW k parameter tunable.
Hybrid blend may down-rank good lexical hits. Mitigation: α default
0.4 leaves lexical at 0.4 weight; --alpha 1.0 falls back to pure
lexical, --alpha 0.0 to pure semantic; expose in
[context_pack].lexical_weight.
Rollback: drop the --lexical-only path → lexical-always; or revert
the whole run_kg_find diff. Vector index stays in Neo4j (harmless).
Pre-flight checklist (run before Commit 1)
cd /home/jd/repos/ARIA-NBV
make check-agent-memory                          # canonical state hygiene
python3 scripts/agents_db.py validate            # backlog schema valid
make kg-status                                   # exit 0 expected
curl -sSf --max-time 1 http://127.0.0.1:11434/api/tags >/dev/null && echo OK || echo "ollama tunnel down — bring it up before proceeding"
curl -sSf --max-time 1 http://127.0.0.1:7474/ >/dev/null && echo "neo4j up" || echo "neo4j DOWN — run make kg-up"
If both tunnels are up: proceed with Commit 1. If only ollama: Commit 1
still works (Phase A doesn't need Neo4j); Commits 2 & 3 wait.

Working-tree hygiene
The repo currently has unrelated other-agent WIP under aria_nbv/...,
docs/contents/impl/... archive renames, and assorted toml/skill edits
(see prior session's git status). When committing, stage only the
litkg/kg files explicitly per commit. Pattern from prior commits in
this branch:

git reset HEAD -- .                                 # clear unrelated stages
git add <paths-touched-by-this-commit>              # explicit list
git diff --cached --stat                            # verify
git commit -m "..."
Critical files (quick reference for the implementation session)
Ranking + tokenizer: .agents/external/litkg-rs/crates/litkg-core/src/ranking.rs
Search call-site for BM25 swap: .agents/external/litkg-rs/crates/litkg-core/src/inspect.rs:1297
CLI run_kg_find: .agents/external/litkg-rs/crates/litkg-cli/src/main.rs:1160
Neo4j sink (papers→nodes): .agents/external/litkg-rs/crates/litkg-neo4j/src/lib.rs:37
Embedding script: .agents/external/litkg-rs/scripts/kg/enrich_embeddings.py (vector index already created at line 502)
Config: .configs/litkg.toml + crates/litkg-core/src/config.rs
Compact filters: scripts/kg/compact_{route,search,claim_check}.jq
Auto-refresh: scripts/kg/auto_refresh.sh (KG_NEO4J_AUTO_UP wired)
Backlog records: .agents/todos.toml (todo-066, 067, 070 plus 062/063/064/065 in parallel)
Reused utilities
Neo4jSink::export pattern (lines 37–186 in litkg-neo4j) — mirror it
for Bolt MERGE.
enrich_embeddings.py — already finds and embeds any KGEmbeddingNode
candidate; reuse without modification.
scripts/kg/ollama_http.py — shell out from Rust for v1 query
embedding to avoid building a Rust client immediately.
is_search_stopword (ranking.rs:92) — extend with stem in the same
pipeline.
auto_refresh.sh ollama probe pattern — re-use shape for the Neo4j
probe inside run_kg_find.
Verification — end-to-end after all three commits
cd /home/jd/repos/ARIA-NBV
make kg-up                                         # ensure Neo4j is up
make kg-refresh-lit                                # JSONL + sync to runtime
make kg-enrich                                     # embed papers + docs + memory
make kg-search KG_QUERY='hierachical viewpoint' KG_FORMAT=json \
  | jq '{mode: .search_mode, fixups: .query_fixups, top: [.results[0:3] | .[].title]}'
# expect: mode=hybrid, fixups contains hierachical→hierarchical, Hestia in top 3.
End-to-end success = the user's original typo'd "hierachical viewpoint"
query (which was the trigger for all of this) finds the Hestia paper
section "Next-Best-View Hierarchical Network" in the top 3, via the
combined BM25 + fuzzy + semantic pipeline.