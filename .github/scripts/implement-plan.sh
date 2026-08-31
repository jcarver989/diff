#!/usr/bin/env bash
set -euo pipefail

pr_number=$(jq -er '.issue.number' "$GITHUB_EVENT_PATH")
pr=$(gh pr view "$pr_number" --repo "$GITHUB_REPOSITORY" \
  --json number,title,body,headRefName,isCrossRepository)
if [[ $(jq -r '.isCrossRepository' <<<"$pr") == true ]]; then
  echo "Aether cannot push to pull request #$pr_number because it comes from a fork" >&2
  exit 1
fi
branch=$(jq -er '.headRefName' <<<"$pr")

gh pr checkout "$pr_number" --repo "$GITHUB_REPOSITORY"
start_sha=$(git rev-parse HEAD)

issue_comments=$(gh api --paginate --slurp "/repos/$GITHUB_REPOSITORY/issues/$pr_number/comments" | jq add)
review_comments=$(gh api --paginate --slurp "/repos/$GITHUB_REPOSITORY/pulls/$pr_number/comments" | jq add)
reviews=$(gh api --paginate --slurp "/repos/$GITHUB_REPOSITORY/pulls/$pr_number/reviews" | jq add)
task_file=.git/aether-task.json
jq -n \
  --arg task "Implement the approved plan in pull request #$pr_number." \
  --argjson pullRequest "$pr" \
  --argjson issueComments "$issue_comments" \
  --argjson reviewComments "$review_comments" \
  --argjson reviews "$reviews" \
  '{kind: "implement_plan", task: $task, pullRequest: $pullRequest, issueComments: $issueComments, reviewComments: $reviewComments, reviews: $reviews}' \
  >"$task_file"
env -u GH_TOKEN -u GITHUB_TOKEN aether headless \
  --agent Orchestrator \
  --cwd "$GITHUB_WORKSPACE" \
  "$task_file"

[[ $(git rev-parse HEAD) != "$start_sha" ]] || exit 0

git push "https://x-access-token:$GH_TOKEN@github.com/$GITHUB_REPOSITORY.git" "HEAD:$branch"
