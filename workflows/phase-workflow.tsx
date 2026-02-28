/** @jsxImportSource smithers-orchestrator */
import React from "react";
import {
  createSmithers,
  Sequence,
  Ralph,
  CodexAgent,
  ClaudeCodeAgent,
} from "smithers-orchestrator";
import { z } from "zod";

const phasePlanSchema = z.object({
  phaseId: z.string(),
  objective: z.string(),
  optimizationResearchPath: z.string(),
  rationaleFromResearch: z.string(),
  implementationDirectives: z.array(z.string()).min(3),
  comparisonTargets: z.array(z.string()).min(2),
  userImpactGoals: z.array(z.string()).min(2),
  readmeUpdatePlan: z.array(z.string()).min(2),
  scope: z.array(z.string()),
  nonGoals: z.array(z.string()),
  acceptanceCriteria: z.array(z.string()),
  implementationSteps: z.array(z.string()),
  testCommands: z.array(z.string()),
  benchmarkCommand: z.string(),
  regressionThresholdPct: z.number(),
  proposedBranch: z.string(),
  proposedPrTitle: z.string(),
});

const implementationSchema = z.object({
  branchName: z.string(),
  commits: z.array(z.string()),
  filesChanged: z.array(z.string()),
  summary: z.string(),
  knownRisks: z.array(z.string()),
});

const validationSchema = z.object({
  testsPassed: z.boolean(),
  benchmarkRan: z.boolean(),
  benchmarkSummary: z.string(),
  benchmarkScoreboardMarkdown: z.string(),
  fastestClaimStatus: z.enum(["supported", "not_supported", "inconclusive"]),
  userImpactSummary: z.string(),
  readmeUpdated: z.boolean(),
  regressionsDetected: z.array(z.string()),
  mergeReady: z.boolean(),
});

const reviewSchema = z.object({
  approved: z.boolean(),
  summary: z.string(),
  issues: z.array(z.string()),
});

const fixesSchema = z.object({
  resolvedIssues: z.array(z.string()),
  unresolvedIssues: z.array(z.string()),
  summary: z.string(),
});

const prPackageSchema = z.object({
  branchName: z.string(),
  prTitle: z.string(),
  prBody: z.string(),
  checklist: z.array(z.string()),
  ghCommands: z.array(z.string()),
});

const releaseGateSchema = z.object({
  approved: z.boolean(),
  note: z.string().nullable(),
});

const { Workflow, Task, smithers, outputs } = createSmithers(
  {
    phase_plan: phasePlanSchema,
    implementation: implementationSchema,
    validation: validationSchema,
    review: reviewSchema,
    fixes: fixesSchema,
    pr_package: prPackageSchema,
    release_gate: releaseGateSchema,
  },
  {
    dbPath: "./.smithers/workflow.db",
  },
);

const codexPlanner = new CodexAgent({
  model: "gpt-5.3-codex",
  cwd: process.cwd(),
  yolo: true,
  instructions:
    "You are a senior engineering planner for Rust performance work. " +
    "Produce pragmatic, phase-scoped plans with explicit validation commands. " +
    "Prioritize correctness, measurable performance wins, user-impact, and minimal risk. " +
    "Every plan must include comparative cache benchmarks and README update steps.",
});

const claudeImplementer = new ClaudeCodeAgent({
  model: "claude-opus-4-6",
  cwd: process.cwd(),
  dangerouslySkipPermissions: true,
  instructions:
    "You are a senior Rust implementation engineer. " +
    "Apply minimal, high-signal changes that satisfy acceptance criteria. " +
    "Always run/interpret tests and benchmarks carefully, avoid API regressions, " +
    "and keep end-user ergonomics/documentation quality high.",
});

const codexReviewer = new CodexAgent({
  model: "gpt-5.3-codex",
  cwd: process.cwd(),
  yolo: true,
  instructions:
    "You are a strict code reviewer focused on correctness and regression prevention. " +
    "Only approve when changes are merge-safe, test-verified, benchmark results do not regress, " +
    "and README/performance claims are evidence-backed and professional.",
});

export default smithers((ctx) => {
  const targetBranch = ctx.input.targetBranch ?? "sr-integration";
  const phaseId = ctx.input.phaseId ?? "phase-1";
  const objective =
    ctx.input.objective ?? "Optimize cache hot paths without regressions";
  const fallbackThreshold = Number(ctx.input.regressionThresholdPct ?? 5);
  const optimizationResearchPath = `${process.cwd()}/OPTIMIZATION_RESEARCH.md`;

  const phasePlan = ctx.outputMaybe("phase_plan", { nodeId: "plan-phase" });
  const implementation = ctx.outputMaybe("implementation", {
    nodeId: "implement-phase",
  });
  const validation = ctx.outputMaybe("validation", {
    nodeId: "validate-phase",
  });
  const review = ctx.outputMaybe("review", { nodeId: "review-phase" });

  const testCommands = Array.isArray(phasePlan?.testCommands)
    ? phasePlan.testCommands
    : ["cargo test --all-features"];
  const benchmarkCommand =
    typeof phasePlan?.benchmarkCommand === "string" &&
    phasePlan.benchmarkCommand.trim().length > 0
      ? phasePlan.benchmarkCommand
      : "cargo bench --no-run";
  const threshold =
    typeof phasePlan?.regressionThresholdPct === "number"
      ? phasePlan.regressionThresholdPct
      : fallbackThreshold;
  const implementationDirectives = Array.isArray(
    phasePlan?.implementationDirectives,
  )
    ? phasePlan.implementationDirectives
    : [];
  const directivesBlock =
    implementationDirectives.length > 0
      ? implementationDirectives
          .map((directive: string, index: number) => `${index + 1}. ${directive}`)
          .join("\n")
      : "1. Follow plan implementation steps conservatively.\n2. Prioritize correctness over speed.\n3. Avoid API regressions.";

  return (
    <Workflow name="phase-pr-workflow">
      <Sequence>
        <Task id="plan-phase" output={outputs.phase_plan} agent={codexPlanner}>
          {`You are planning ONE optimization phase for this repository.

Input:
- phaseId: ${phaseId}
- objective: ${objective}
- targetBranch: ${targetBranch}
- regressionThresholdPct: ${fallbackThreshold}

Research Input:
- Read and use this file as a primary planning source: ${optimizationResearchPath}

Rules:
1. Keep scope small enough for one PR.
2. Preserve existing public API unless explicitly required.
3. Include exact commands for validation.
4. Benchmark command must be realistic for this repo.
5. proposedBranch format: optimization/<phaseId>-<short-topic>
6. Derive concrete implementationDirectives for Claude from OPTIMIZATION_RESEARCH.md.
7. Define at least 2 comparisonTargets (existing Rust caches) for benchmark scoring.
8. Include userImpactGoals focused on real end-user outcomes (API clarity, predictability, reliability).
9. Include a readmeUpdatePlan that keeps README performance claims precise and professional.

Required output guidance:
- optimizationResearchPath must match the path above.
- rationaleFromResearch should summarize which findings from OPTIMIZATION_RESEARCH.md drove your decisions.
- implementationDirectives should be clear, actionable instructions the implementer can follow directly.
- acceptanceCriteria must include:
  - comparative benchmark scoring versus comparisonTargets
  - user-centered acceptance checks
  - README update completion criteria

Return only structured output matching schema.`}
        </Task>

        <Ralph
          id="implement-review-loop"
          until={ctx.latest("review", "review-phase")?.approved === true}
          maxIterations={4}
          onMaxReached="fail"
        >
          <Sequence>
            <Task
              id="implement-phase"
              output={outputs.implementation}
              agent={claudeImplementer}
            >
              {`Implement the planned phase in this repo.

Plan:
${JSON.stringify(phasePlan ?? { note: "Plan not available yet." }, null, 2)}

Requirements:
- Work on branch: ${phasePlan?.proposedBranch ?? `optimization/${phaseId}-work`}
- Follow implementationSteps exactly unless blocked.
- Follow these planner directives derived from OPTIMIZATION_RESEARCH.md:
${directivesBlock}
- Run tests and checks incrementally during implementation.
- Keep commits focused and reversible.
- Implement and collect comparative benchmark metrics against:
${Array.isArray(phasePlan?.comparisonTargets) && phasePlan.comparisonTargets.length > 0 ? phasePlan.comparisonTargets.map((target: string, i: number) => `${i + 1}. ${target}`).join("\n") : "1. Include at least two relevant Rust cache libraries in the benchmark comparison."}
- Present benchmark outcomes so it is clear whether micro-moka is the fastest single-threaded option for tested scenarios.
- Apply user-centered design constraints from plan userImpactGoals and avoid changes that hurt ergonomics/usability.
- Update README according to readmeUpdatePlan with professional wording and reproducible benchmark instructions/results.
- If this is a re-run, include fixes for prior review feedback.

Prior review feedback (if any):
${review?.approved === false ? JSON.stringify(review, null, 2) : "none"}

Return only structured output matching schema.`}
            </Task>

            <Task
              id="validate-phase"
              output={outputs.validation}
              agent={claudeImplementer}
            >
              {`Validate the implementation for this phase.

Plan:
${JSON.stringify(phasePlan ?? { note: "Plan not available yet." }, null, 2)}

Implementation summary:
${JSON.stringify(implementation ?? { note: "Implementation not available yet." }, null, 2)}

Run these commands and report results:
- Tests: ${testCommands.join(" && ")}
- Benchmark: ${benchmarkCommand}

Set mergeReady=true only if:
- testsPassed is true
- benchmarkRan is true
- regressionsDetected is empty (threshold ${threshold}%)
- benchmarkScoreboardMarkdown clearly compares micro-moka vs comparisonTargets
- fastestClaimStatus is not "inconclusive"
- userImpactSummary confirms end-user experience was preserved or improved
- readmeUpdated is true and aligned with measured outcomes

Return only structured output matching schema.`}
            </Task>

            <Task id="review-phase" output={outputs.review} agent={codexReviewer}>
              {`Review this phase for merge readiness.

Plan:
${JSON.stringify(phasePlan ?? { note: "Plan not available yet." }, null, 2)}

Implementation:
${JSON.stringify(implementation ?? { note: "Implementation not available yet." }, null, 2)}

Validation:
${JSON.stringify(validation ?? { note: "Validation not available yet." }, null, 2)}

Rules:
- Approve only if this is safe to merge into ${targetBranch}.
- If not approved, provide concrete, actionable issues.
- Be strict on correctness, tests, and regression risk.
- Reject if benchmarkScoreboardMarkdown does not substantiate claims.
- Reject if fastestClaimStatus is "not_supported" or "inconclusive" without corrective follow-up.
- Reject if userImpactSummary indicates degraded end-user experience.
- Reject if README updates are missing, misleading, or unprofessional.

Return only structured output matching schema.`}
            </Task>

            <Task
              id="fix-review-issues"
              output={outputs.fixes}
              agent={claudeImplementer}
              skipIf={ctx.latest("review", "review-phase")?.approved === true}
            >
              {`Resolve all review issues.

Latest review:
${JSON.stringify(review ?? { note: "Review not available yet." }, null, 2)}

Plan:
${JSON.stringify(phasePlan ?? { note: "Plan not available yet." }, null, 2)}

Validation summary:
${JSON.stringify(validation ?? { note: "Validation not available yet." }, null, 2)}

Apply the smallest safe set of changes to resolve issues.
Return only structured output matching schema.`}
            </Task>
          </Sequence>
        </Ralph>

        <Task
          id="prepare-pr-package"
          output={outputs.pr_package}
          agent={codexPlanner}
        >
          {`Prepare PR artifacts for this completed phase.

Plan:
${JSON.stringify(phasePlan ?? { note: "Plan not available yet." }, null, 2)}

Implementation:
${JSON.stringify(implementation ?? { note: "Implementation not available yet." }, null, 2)}

Validation:
${JSON.stringify(validation ?? { note: "Validation not available yet." }, null, 2)}

Final review:
${JSON.stringify(review ?? { note: "Review not available yet." }, null, 2)}

Generate:
- prTitle
- prBody
- checklist
- ghCommands (push branch, create PR to ${targetBranch})

PR body/checklist must explicitly include:
- benchmarkScoreboardMarkdown
- whether fastestClaimStatus is supported with evidence
- user-impact notes for downstream consumers
- README updates and reproducibility notes

Return only structured output matching schema.`}
        </Task>

        <Task id="release-gate" output={outputs.release_gate} needsApproval>
          {{
            approved: false,
            note:
              "Manual gate before merge/release. Approve after PR review and required checks pass.",
          }}
        </Task>
      </Sequence>
    </Workflow>
  );
});
