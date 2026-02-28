import {
  createSmithers,
  Task,
  Sequence,
  Parallel,
  Ralph,
  runWorkflow,
  ClaudeCodeAgent,
} from "smithers-orchestrator";
import { z } from "zod";

const { Workflow, smithers, outputs } = createSmithers({
  research: z.object({
    all_done: z.boolean(),
    optimization: z.string(),
    description: z.string(),
    files_to_modify: z.array(z.string()),
    implementation_plan: z.string(),
  }),
  implementation: z.object({
    branch_name: z.string(),
    version: z.string(),
    files_changed: z.array(z.string()),
    summary: z.string(),
  }),
  pr: z.object({
    pr_number: z.number(),
    pr_url: z.string(),
  }),
  claude_review: z.object({
    approved: z.boolean(),
    feedback: z.string(),
  }),
  codex_review: z.object({
    approved: z.boolean(),
    feedback: z.string(),
  }),
  merge: z.object({
    merged: z.boolean(),
    merge_sha: z.string(),
  }),
  release_check: z.object({
    version: z.string(),
    crate_published: z.boolean(),
    release_url: z.string(),
  }),
});

const claude = new ClaudeCodeAgent();

function completedOptimizations(ctx: any): string[] {
  const names: string[] = [];
  for (let i = 0; i < 20; i++) {
    const r = ctx.outputMaybe("research", { nodeId: "research", iteration: i });
    if (!r) break;
    if (r.optimization) names.push(r.optimization);
  }
  return names;
}

const workflow = smithers((ctx) => {
  const done = completedOptimizations(ctx);
  const latestResearch = ctx.outputMaybe("research", { nodeId: "research" });
  const allDone = latestResearch?.all_done === true;

  return (
    <Workflow name="optimization-pipeline">
      <Ralph until={allDone} maxIterations={20} onMaxReached="return-last">
        <Sequence>
          <Task id="research" output={outputs.research} agent={claude}>
            {`Read the file OPTIMIZATION_RESEARCH.md in the current repository.

Analyze ALL optimization recommendations across all tiers and phases.

${
  done.length > 0
    ? `The following optimizations have ALREADY been completed in previous iterations. Do NOT select any of these again:
${done.map((name, i) => `${i + 1}. ${name}`).join("\n")}

Select the next highest-impact optimization that has NOT been done yet.`
    : `This is the first iteration. Select the single highest-impact optimization
that can be implemented as an isolated, self-contained change.

Prefer optimizations from "Tier 3: Hot Path Optimization" or "Phase 1: Quick Wins"
as these are lower risk and can be done independently without large refactors.`
}

If ALL worthwhile optimizations from OPTIMIZATION_RESEARCH.md have been completed,
set all_done to true and fill the other fields with "N/A" / empty values.

Return JSON with:
- all_done: false if there are more optimizations to do, true if all are complete
- optimization: name of the optimization (or "N/A" if all_done)
- description: detailed description of the changes needed
- files_to_modify: list of file paths that will need modification
- implementation_plan: step-by-step implementation plan`}
          </Task>

          <Task
            id="implement"
            output={outputs.implementation}
            agent={claude}
            skipIf={allDone}
          >
            {(() => {
              const research = ctx.outputMaybe("research", {
                nodeId: "research",
              });
              if (!research || research.all_done) return "Pending";
              return `Implement the following optimization in the micro-moka Rust codebase:

Optimization: ${research.optimization}
Description: ${research.description}
Files to modify: ${research.files_to_modify.join(", ")}
Implementation plan: ${research.implementation_plan}

You MUST also do the following release prep:
1. Bump the patch version in Cargo.toml (current version is read from the file)
2. Add a changelog entry in CHANGELOG.md with the format:
   ## [x.y.z] - 2026-02-28
   - <description of the optimization>

Implementation steps:
1. Create a new git branch named after the optimization
2. Implement the optimization following the plan
3. Write unit tests for any changed behavior
4. Run: cargo clippy --lib --tests --all-features --all-targets -- -D warnings
5. Run: RUSTFLAGS='--cfg trybuild' cargo test --all-features
6. Ensure all tests pass before finishing
7. Commit all changes

Return JSON with:
- branch_name: the git branch name
- version: the new version string
- files_changed: list of files changed
- summary: summary of what was done`;
            })()}
          </Task>

          <Task
            id="push-pr"
            output={outputs.pr}
            agent={claude}
            skipIf={allDone}
          >
            {(() => {
              const impl = ctx.outputMaybe("implementation", {
                nodeId: "implement",
              });
              if (!impl) return "Pending";
              return `Push the branch "${impl.branch_name}" and create a pull request.

Run these commands:
1. git push -u origin ${impl.branch_name}
2. gh pr create --base main --head ${impl.branch_name} \\
     --title "<concise title describing the optimization>" \\
     --body "<summary including: what optimization, why, files changed, test results>"

Summary of changes: ${impl.summary}
Files changed: ${impl.files_changed.join(", ")}
New version: ${impl.version}

Return JSON with:
- pr_number: the PR number
- pr_url: the full PR URL`;
            })()}
          </Task>

          <Parallel>
            <Task
              id="claude-review"
              output={outputs.claude_review}
              agent={claude}
              skipIf={allDone}
            >
              {(() => {
                const pr = ctx.outputMaybe("pr", { nodeId: "push-pr" });
                if (!pr) return "Pending";
                return `Review pull request #${pr.pr_number} at ${pr.pr_url}.

Run: gh pr diff ${pr.pr_number}

Check for:
- Correctness of the implementation
- No regressions or unsafe code issues
- Tests cover the changes adequately
- Code follows existing conventions (no unnecessary comments, no emojis)
- Version bump in Cargo.toml and CHANGELOG.md entry present

If everything looks good, approve:
  gh pr review ${pr.pr_number} --approve --body "LGTM"

If issues found, request changes:
  gh pr review ${pr.pr_number} --request-changes --body "<feedback>"

Return JSON with:
- approved: boolean
- feedback: your review feedback`;
              })()}
            </Task>

            <Task
              id="codex-review"
              output={outputs.codex_review}
              agent={claude}
              skipIf={allDone}
            >
              {(() => {
                const pr = ctx.outputMaybe("pr", { nodeId: "push-pr" });
                if (!pr) return "Pending";
                return `Review pull request #${pr.pr_number} at ${pr.pr_url}.

Run: gh pr diff ${pr.pr_number}

Check for:
- Performance implications of the changes
- Memory safety concerns
- Algorithm correctness
- Edge cases that might not be covered

If everything looks good, approve:
  gh pr review ${pr.pr_number} --approve --body "LGTM"

If issues found, request changes:
  gh pr review ${pr.pr_number} --request-changes --body "<feedback>"

Return JSON with:
- approved: boolean
- feedback: your review feedback`;
              })()}
            </Task>
          </Parallel>

          <Task
            id="merge-pr"
            output={outputs.merge}
            agent={claude}
            skipIf={allDone}
          >
            {(() => {
              const pr = ctx.outputMaybe("pr", { nodeId: "push-pr" });
              const claudeReview = ctx.outputMaybe("claude_review", {
                nodeId: "claude-review",
              });
              const codexReview = ctx.outputMaybe("codex_review", {
                nodeId: "codex-review",
              });
              if (!pr || !claudeReview || !codexReview) return "Pending";
              const bothApproved = claudeReview.approved && codexReview.approved;
              return `Review status for PR #${pr.pr_number}:
- Claude: ${claudeReview.approved ? "APPROVED" : "CHANGES REQUESTED"} - ${claudeReview.feedback}
- Codex: ${codexReview.approved ? "APPROVED" : "CHANGES REQUESTED"} - ${codexReview.feedback}

${
  bothApproved
    ? `Both reviewers approved. Merge the PR now:
  gh pr merge ${pr.pr_number} --merge --delete-branch`
    : `Not all reviewers approved. Do NOT merge.`
}

Return JSON with:
- merged: boolean (true only if merge succeeded)
- merge_sha: the merge commit SHA (or empty string if not merged)`;
            })()}
          </Task>

          <Task
            id="verify-release"
            output={outputs.release_check}
            agent={claude}
            skipIf={allDone}
          >
            {(() => {
              const impl = ctx.outputMaybe("implementation", {
                nodeId: "implement",
              });
              const merge = ctx.outputMaybe("merge", { nodeId: "merge-pr" });
              if (!impl || !merge) return "Pending";
              return `The PR was ${merge.merged ? "merged" : "NOT merged"}.

${
  merge.merged
    ? `After merging to main, the publish-crate.yml workflow should automatically:
1. Publish the crate to crates.io
2. Create a git tag v${impl.version}
3. Create a GitHub release

Verify:
1. Check for the GitHub Actions workflow run:
   gh run list --workflow=publish-crate.yml --limit 3
2. Wait for it to complete if still running:
   gh run watch <run-id>
3. Check the release was created:
   gh release list --limit 5
4. Confirm the version tag exists:
   git ls-remote --tags origin | grep v${impl.version}

Return JSON with:
- version: "${impl.version}"
- crate_published: true if the workflow succeeded
- release_url: the GitHub release URL (from gh release view v${impl.version} --json url)`
    : `The PR was not merged, so no release is expected.

Return JSON with:
- version: "${impl.version}"
- crate_published: false
- release_url: ""`
}`;
            })()}
          </Task>
        </Sequence>
      </Ralph>
    </Workflow>
  );
});

export default workflow;

if (import.meta.main) {
  const result = await runWorkflow(workflow, {
    input: {},
    onProgress: (event) => {
      const ts = new Date().toISOString().slice(11, 19);
      switch (event.type) {
        case "NodeStarted":
          console.log(`[${ts}] >> ${event.nodeId} (iteration ${event.iteration})`);
          break;
        case "NodeFinished":
          console.log(`[${ts}] << ${event.nodeId} done (iteration ${event.iteration})`);
          break;
        case "NodeFailed":
          console.log(`[${ts}] !! ${event.nodeId} failed: ${event.error}`);
          break;
      }
    },
  });

  console.log(JSON.stringify(result, null, 2));
}
