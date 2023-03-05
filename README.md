
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

## Approach

Uses the `gh` tool to fetch details about all open pull-requests. Uses the `git` command line program and the Rust
`libgit2` bindings to fetch the corresponding branches for the pull requests from their source repos and then analyse
the diffs of the branches against the common ancester with their target branch. The analysis of the diffs focuses on
the details of the 'before' files so that clashes can be identified based on what has been edited in the current files.

Displays a heatmap against the current state of the repository. File name differences or new files between the older
state of branches and the current state of the repo are not handled.

The heatmap is an HTML list of files names with blocks of colour indicating the frequency with which files are changed
in pull requests.

## Known Issues

- One fetched PR has a null for one of the fields and needs to be deleted from the json before processing.
- The heatmap is generated on the current state of the repo. Files that are changed in pull requests which no longer
  exists at that path name in the current repo are not handled or displayed.
