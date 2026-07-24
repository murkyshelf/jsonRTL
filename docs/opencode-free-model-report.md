# Report: Using Free Models with OpenCode

Date: 2026-07-22

## Summary

OpenCode can run delegated coding tasks against free models by selecting a model with `--model`. In this workspace, free models are useful for bounded implementation work, test expansion, mechanical refactors, and first-pass documentation. They should not be treated as autonomous final reviewers for production code. The most reliable workflow is to give them a narrow specification, require exact verification commands, inspect their output, and send corrective tasks when the implementation drifts from the contract.

The recent Phase 4 REST work showed the practical tradeoff. Free OpenCode models were able to produce meaningful API implementation and test coverage, but they also introduced contract mistakes, unnecessary dependencies, and a few hallucinated type assumptions. The final result became acceptable only after manager review, targeted corrective prompts, and independent verification.

## Local Environment

Observed OpenCode version:

```sh
opencode --version
# 1.18.4
```

Model discovery command:

```sh
opencode models
```

Free models visible in this environment included:

- `opencode/deepseek-v4-flash-free`
- `opencode/laguna-s-2.1-free`
- `opencode/mimo-v2.5-free`
- `opencode/nemotron-3-ultra-free`
- `opencode/north-mini-code-free`
- `openrouter/cohere/north-mini-code:free`

The list is environment-dependent. Treat `opencode models` as the authoritative source for the current machine rather than assuming a fixed catalog.

## Usage Pattern

Use `opencode run` with an explicit model and working directory:

```sh
opencode run --auto \
  --model opencode/deepseek-v4-flash-free \
  --dir /path/to/repo \
  'Implement the bounded task described here...'
```

Useful flags:

- `--model`: selects the provider/model pair.
- `--dir`: anchors the agent in the intended repository.
- `--auto`: allows OpenCode to apply edits without stopping for routine approvals. Use only when the task is scoped and reviewable.
- `--continue` or `--session`: resumes a prior OpenCode session when a corrective task should keep context.

## Effective Task Shape

Free models perform best when the prompt is concrete and bounded. A good delegation includes:

- Objective: one deliverable, not a whole phase.
- Repository context: real module names, ownership boundaries, and existing APIs.
- Files likely to change: small set where possible.
- Constraints: compatibility, no unrelated edits, no new dependencies unless justified.
- Acceptance criteria: wire shapes, status codes, invariants, and test expectations.
- Verification: exact commands the model must run before returning.

Large prompts are still sometimes necessary, but the work should be split into independently reviewable pieces. Free models are more likely to drift when asked to design, implement, test, and operationalize a broad feature in one pass.

## Observed Strengths

Free OpenCode models were useful for:

- Creating a broad first implementation quickly.
- Adding API tests around success and failure paths.
- Running normal Rust verification commands.
- Applying targeted corrections once the exact defect was described.
- Handling mechanical edits such as response-shape changes and dependency cleanup.

They were most effective when the prompt named the exact wrong behavior and the expected replacement.

## Observed Weaknesses

The same work exposed several risks:

- Contract drift: a model flattened an error envelope that required a nested `error` object.
- Architecture drift: a core diagnostic constructor was widened instead of keeping transport code out of kernel internals.
- Dependency drift: unnecessary dependencies were added for request IDs and body handling.
- Weak tests: initial tests checked status codes but missed body/header request ID equality and exact response shape.
- Hallucinated APIs: one fallback free model invented nonexistent type methods and struct fields after insufficient type inspection.
- False completion signals: green tests were not enough when tests encoded the wrong contract.

These weaknesses are manageable, but only if OpenCode output is treated as a draft until reviewed against the actual specification.

## Recommended Workflow

1. Inspect the repository and current task source before delegation.
2. Send a bounded implementation spec to OpenCode with a free model.
3. Require the model to run formatting, linting, tests, and any version-specific checks.
4. Review source code directly, not just the model summary.
5. Search for known risk patterns such as unnecessary dependencies, public API widening, `unwrap`, `expect`, weak tests, and fabricated adapters.
6. Send corrective tasks instead of rewriting large sections manually when working in a manager/delegation workflow.
7. Run independent verification after OpenCode finishes.
8. Record the accepted result and any remaining limitations.

## Practical Guidance

Prefer free models for:

- Bounded code changes with explicit acceptance criteria.
- Regression test additions.
- Documentation drafts.
- Mechanical refactors where the target shape is clear.
- Exploratory implementation that will receive strict review.

Avoid relying on free models alone for:

- Security-sensitive boundary code.
- Public API design without review.
- Cross-crate architecture decisions.
- Subtle protocol or wire-contract work.
- Final acceptance of a task.

For high-risk code, a free model can still be useful, but its output should go through a senior review loop and independent tests that prove the actual contract.

## Conclusion

Using a free model on OpenCode is a good productivity tool when the task is narrow, the specification is precise, and final acceptance remains human or manager-controlled. It is not a substitute for repository inspection, contract review, or independent verification. In this project, free models moved the REST implementation forward materially, but correctness came from iterative correction and verification rather than from the first generated pass.
