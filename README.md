
# Public Relations

Exploratory repo for analysing pull requests on a project.

## Requirements

Uses the [gh](https://cli.github.com/) GitHub command line tool to fetch information about the PRs.

## Running

Clone this repo, then:

```bash
cd public-relations
mkdir -p repos
git clone git@github.com:helix-editor/helix.git repos/helix

cd repos/helix
gh pr list --limit 500 --json baseRefName --json headRefName --json headRefOid --json headRepository --json headRepositoryOwner --json number --json id > ../../prs.json
cd ../../

# Format the json as it makes it easier to remove entries that don't wory (only two at time of testing)
cat prs.json | python3 -m json.tool  > prs-formatted.json

# Will take a while to fetch all 300+ pull request branches
cargo run repos/helix prs-formatted.json
```

