# Agent Guidelines

DOT NOT TO READ THE FILE IN `.hidden`

## Development Documentation

Use these documents as the project map before making architectural, runtime, UI, or workflow changes:

- `README.md` — install, setup, service, gateway, and user-facing CLI behavior.
- `SKILL.md` — installed-skill operating contract for agents.
- `references/agent-usage.md` — agent usage policy, search/write rules, visual inspection, and global/project memory boundaries.
- `references/configuration.md` — `memory.yaml`, runtime config, Qdrant storage, and service configuration.
- `DESIGN/DESIGN.md` — design index and current public interface map.
- `DESIGN/01-cli-and-workflows.md` — CLI flow and command contracts.
- `DESIGN/02-configuration-and-discovery.md` — config, discovery, logical database naming, and AGENTS.md injection.
- `DESIGN/03-storage-and-schema.md` — Qdrant schema and record payload.
- `DESIGN/04-search-embedding-worker.md` — search, embeddings, scoring, and worker behavior.
- `DESIGN/05-service-ui-scripts.md` — resident service, gateway main process, Qdrant dashboard proxy, removable-disk handling, and install scripts.

## Documentation Maintenance

When a change meaningfully alters product behavior, architecture, runtime configuration, persistence, testing expectations, or gateway/UI behavior, update the relevant `README.md`, `SKILL.md`, `references/`, or `DESIGN/` document in the same change set. Do not let implementation and documentation drift.

## Overthinking and excessive thoroughness

When you're deciding how to approach a problem, choose an approach and commit to it. Avoid revisiting decisions unless you encounter new information that directly contradicts your reasoning. If you're weighing two approaches, pick one and see it through. You can always course-correct later if the chosen approach fails.

After receiving tool results, carefully reflect on their quality and determine optimal next steps before proceeding. Use your thinking to plan and iterate based on this new information, and then take the best next action.

Avoid over-engineering. Only make changes that are directly requested or clearly necessary. Keep solutions simple and focused:

- Scope: Don't add features, refactor code, or make "improvements" beyond what was asked. A bug fix doesn't need surrounding code cleaned up. A simple feature doesn't need extra configurability.

- Documentation: Don't add docstrings, comments, or type annotations to code you didn't change. Only add comments where the logic isn't self-evident.

- Defensive coding: Don't add error handling, fallbacks, or validation for scenarios that can't happen. Trust internal code and framework guarantees. Only validate at system boundaries (user input, external APIs).

- Abstractions: Don't create helpers, utilities, or abstractions for one-time operations. Don't design for hypothetical future requirements. The right amount of complexity is the minimum needed for the current task.

## Reduce file creation in agentic coding

If you create any temporary new files, scripts, or helper files for iteration, clean up these files by removing them at the end of the task.

## Avoid focusing on passing tests and hard-coding

Please write a high-quality, general-purpose solution using the standard tools available. Do not create helper scripts or workarounds to accomplish the task more efficiently. Implement a solution that works correctly for all valid inputs, not just the test cases. Do not hard-code values or create solutions that only work for specific test inputs. Instead, implement the actual logic that solves the problem generally.

Focus on understanding the problem requirements and implementing the correct algorithm. Tests are there to verify correctness, not to define the solution. Provide a principled implementation that follows best practices and software design principles.

If the task is unreasonable or infeasible, or if any of the tests are incorrect, please inform me rather than working around them. The solution should be robust, maintainable, and extendable.

<investigate_before_answering>
Never speculate about code you have not opened. If the user references a specific file, you MUST read the file before answering. Make sure to investigate and read relevant files BEFORE answering questions about the codebase. Never make any claims about code before investigating unless you are certain of the correct answer - give grounded and hallucination-free answers.
</investigate_before_answering>

## Agent Memory

<!-- agent-memory:config:start -->
Agent Memory configuration: `.memory/memory.yaml`.

Agent Memory magic word:
- If the user writes `$agent_memory init`, run `agent-memory --agent init --start-service` for the current project, then report the service status.

Agent Memory usage:
- Before starting a non-trivial task, run `agent-memory --agent memory discover` to load the active memory configuration.
- Search memory before asking the user when prior decisions, repository conventions, user preferences, known failures, domain knowledge, or research may matter: `agent-memory --agent memory search "<query>"`.
- Treat memory as advisory. Current user instructions, live repository contents, official documentation, and fresh tool output override stored memory.
- After making a durable, reusable, evidence-backed discovery, consider writing it with `agent-memory --agent memory add --content "<memory>" --type <type> --source-kind <kind> --source-ref <ref> --confidence <0..1> --keys "<search keys>"`.
- Run embedding work with `agent-memory --agent service worker --once`, or keep the resident service available with `agent-memory --agent service start` and stop it with `agent-memory --agent service stop`.
- Write project-specific memories to project scope and global/user-preference memories to global scope when a global memory config is available.
- If only project memory is configured, write otherwise-global relevant memories to the project memory instead of dropping them.
<!-- agent-memory:config:end -->
