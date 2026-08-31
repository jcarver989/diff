#!/usr/bin/env bash
set -euo pipefail

issue_number=$(jq -er '.issue.number' "$GITHUB_EVENT_PATH")
branch="aether/issue-$issue_number"
if git show-ref --verify --quiet "refs/remotes/origin/$branch"; then
  git switch --force-create "$branch" "origin/$branch"
else
  git switch --create "$branch" "origin/$DEFAULT_BRANCH"
fi
start_sha=$(git rev-parse HEAD)

task_file=.git/aether-task.json
jq '{kind: "issue", issue: (.issue | {number, title, body, labels: [.labels[].name]})}' \
  "$GITHUB_EVENT_PATH" >"$task_file"
env -u GH_TOKEN -u GITHUB_TOKEN aether headless \
  --agent Orchestrator \
  --cwd "$GITHUB_WORKSPACE" \
  "$task_file"

[[ $(git rev-parse HEAD) != "$start_sha" ]] || exit 0

git push "https://x-access-token:$GH_TOKEN@github.com/$GITHUB_REPOSITORY.git" "HEAD:$branch"
open_prs=$(gh pr list --repo "$GITHUB_REPOSITORY" --head "$branch" --state open \
  --json number --jq length)
if [[ "$open_prs" == 0 ]]; then
  gh pr create --repo "$GITHUB_REPOSITORY" \
    --base "$DEFAULT_BRANCH" \
    --head "$branch" \
    --title "$(git log -1 --format=%s)" \
    --body "Closes #$issue_number"
fi
