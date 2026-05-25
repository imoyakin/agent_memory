# Agent Guidelines

DOT NOT TO READ THE FILE IN `.hidden`

## Development Documentation

Use these documents as the project map before making architectural, runtime, UI, or workflow changes:

- `docs/README.md` — documentation index.
- `docs/product/information-architecture.md` — product scope and information architecture.
- `docs/interfaces/macos-swiftui-interface.md` — macOS SwiftUI interface structure and navigation.
- `docs/interfaces/cli-agent-control.md` — CLI and agent-control surfaces.
- `docs/core/control-layer.md` — core lifecycle and control boundaries.
- `docs/core/mihomo-runtime-controller.md` — Mihomo runtime controller behavior.
- `docs/core/profiles-runtime-configuration.md` — profile import, metadata, and runtime configuration flow.
- `docs/operations/system-integration-permissions.md` — macOS permissions and system integration.
- `docs/operations/persistence-logging.md` — persistence, state files, and logging.
- `docs/operations/release-management.md` — release artifacts and app update flow.
- `docs/roadmap/service-mode-roadmap.md` — service-mode direction and staged roadmap.
- `docs/quality/testing-quality.md` — testing and quality expectations.

SwiftUI-specific implementation guidance lives under `.agents/skills/`, especially:

- `.agents/skills/swiftui-expert-skill/SKILL.md`
- `.agents/skills/macos-design-guidelines/SKILL.md`
- `.agents/skills/swiftui-liquid-glass/SKILL.md`
- `.agents/skills/liquid-glass-design/SKILL.md`
- `.agents/skills/swiftui-animation/SKILL.md`
- `.agents/skills/swiftui-ui-patterns/SKILL.md`
- `.agents/skills/swiftui-view-refactor/SKILL.md`
- `.agents/skills/swiftui-performance-audit/SKILL.md`

## Documentation Maintenance

When a change meaningfully alters product behavior, architecture, runtime configuration, persistence, permissions, testing expectations, or UI information architecture, update the relevant document in `docs/` in the same change set. Do not let implementation and documentation drift.

## UI Copy Constraints

- Avoid redundant copy. Do not repeat information already expressed by a title, metric, selected state, icon, or surrounding section.
- Prefer concise labels over explanatory text when the UI state is self-evident.
- Remove disabled placeholder actions unless they teach a real next step.
- Do not add low-information detail text such as "Current profile", "selected", or repeated counts when nearby UI already communicates the same fact.
- Keep user-visible copy in English unless explicitly asked otherwise.

## SwiftUI Native Component Constraints

- Prefer native SwiftUI and macOS controls (`NavigationSplitView`, `List`, `Table`, `Form`, `Menu`, `Button`, `Picker`, `Toggle`, `PasteButton`) before custom components.
- Do not recreate native selection, toolbar, menu, sidebar, or button behavior with overlays, fake masks, or hand-rolled hit targets.
- For Liquid Glass, prefer native APIs (`glassEffect`, `GlassEffectContainer`, `glassEffectID`, `.buttonStyle(.glass)`, `.buttonStyle(.glassProminent)`) and apply interactive glass only to interactive elements.
- Keep custom views small and compositional. Extract only when it clarifies state, layout, or reuse.

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
Agents should load this memory.yaml before using the agent-memory skill.

Agent Memory operating rules:
- Before starting a task, discover the active memory config and search for relevant prior memory.
- While analyzing the user's request and again before finishing, identify whether any durable, reusable memory should be written.
- Write project-specific memories to project scope and global/user-preference memories to global scope when a global memory config is available.
- If only project memory is configured, write otherwise-global relevant memories to the project memory instead of dropping them.
<!-- agent-memory:config:end -->
