---
name: gh-request-copilot-review
description: Request a PR review from the Github Copilot reviewer bot.
license: MIT
compatibility: Requires gh CLI and Copilot PR Reviewer app installed on the repo.
metadata:
  author: opencode
  version: "1.0"
  generatedBy: "opencode"
---

## Skill: gh-request-copilot-review

Request a pull request review from the Github Copilot reviewer bot.

**Command**:
```bash
gh api -X POST repos/{owner}/{repo}/pulls/{number}/requested_reviewers -f "reviewers[]=copilot-pull-request-reviewer[bot]"
```

**Usage Instructions**:

1. **Identify the Repository and PR Number**:
   - If not provided, you can find the repository with:
     ```bash
     gh repo view --json owner,name --jq '.owner.login + "/" + .name'
     ```
   - If not provided, you can find the current PR number for the active branch with:
     ```bash
     gh pr view --json number --jq '.number'
     ```

2. **Execute the Request**:
   - Run the following command, replacing `{owner}`, `{repo}`, and `{number}` with the values identified above:
     ```bash
     gh api -X POST repos/{owner}/{repo}/pulls/{number}/requested_reviewers -f "reviewers[]=copilot-pull-request-reviewer[bot]"
     ```

3. **Verify**:
   - You can verify the request was sent by checking the PR's review requests:
     ```bash
     gh pr view {number} --json reviewRequests
     ```

**Guardrails**:
- Only use this if the repository has the Copilot PR Reviewer app installed.
- Ensure the PR number is correct for the current context.
