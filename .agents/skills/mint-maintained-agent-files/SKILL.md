---
name: mint-maintained-agent-files
description: Maintain MINT agent instructions, repository skills and architecture-document routing without duplicate rules, stale paths or unsupported workflow claims.
---

# Agent files and documentation

Keep always-applicable constraints in [AGENTS.md](../../../AGENTS.md), task workflows in `.agents/skills`, and behavior/ownership contracts in [docs/architecture](../../../docs/architecture). Keep transient task plans out of durable guidance.

1. Inventory the affected rules, routes, source owners, tests, and documentation. Preserve the user's active scope and existing authorization; do not add new approval gates based on generic caution.
2. Use the skill-creator guidance when available for new or substantially changed skills. Keep each skill instruction-only unless a concrete reusable resource is needed. Consult current official product documentation before making claims about agent discovery or tool configuration.
3. Give each fact one authoritative owner and link to it. Read only relevant contract sections for bounded changes. Distinguish implemented behavior, proposed work, and verified external settings.
4. Keep one root `AGENTS.md`; do not add nested or tool-specific parallel instruction systems. New skill directories need root routing and valid links. Technical prose is English and must not include personal machine paths, credentials, or copied private project policies.
5. Repository skills use a deliberately simple frontmatter subset, with single-line plain `name` and `description` fields. If richer metadata becomes necessary, extend the validator deliberately rather than silently bypassing it.
6. Run `python scripts/check_agent_guidance.py` and `python -m unittest discover -s scripts/tests`. When available, also run the skill-creator validator for each changed skill. Inspect the final diff and root validation requirements; guidance-only changes do not require rebuilding Rust.

The structural check validates encoding, local file links, routing, metadata and a single instruction entry point. It does not prove that an instruction is useful or a behavioral claim is true; review those against code and realistic tasks.
