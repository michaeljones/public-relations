///! Runs with a path to a git repo and a json file build with:
///!
///!   gh pr list --limit 500 --json baseRefName --json headRefName --json headRefOid --json headRepository --json headRepositoryOwner --json number --json id > prs.json
///!
///! With problematic entries manually deleted (one with null as the repo due to the fork being deleted)
use std::{collections::HashMap, path::PathBuf};

use anyhow::Context;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PullRequest {
    id: String,
    number: u32,

    base_ref_name: String,

    head_ref_name: String,
    head_ref_oid: String,
    head_repository: Repo,
    head_repository_owner: User,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Repo {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct User {
    login: String,
}

fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().collect();

    let repo_path = PathBuf::from(
        args.get(1)
            .context("Usage: cargo run <repo path> <json path>")?,
    );
    let json_path = PathBuf::from(
        args.get(2)
            .context("Usage: cargo run <repo path> <json path>")?,
    );

    println!("{}", json_path.display());

    let json_string = std::fs::read_to_string(&json_path)?;
    let data: Vec<PullRequest> = serde_json::from_str(&json_string)?;

    for pull_request in data.iter() {
        let user = &pull_request.head_repository_owner.login;
        let repo = &pull_request.head_repository.name;
        let from_branch = &pull_request.head_ref_name;
        let to_branch = format!("pull-request-{}", pull_request.number);

        // git fetch git@github.com:cessen/helix set_tab_width:pull-request-561
        let origin = format!("git@github.com:{user}/{repo}");
        let from_to = format!("{from_branch}:{to_branch}");

        let output = std::process::Command::new("git")
            .args(["fetch", &origin, &from_to])
            .current_dir(&repo_path)
            .output()?;

        if !output.status.success() {
            eprintln!("Failed to run git fetch for {origin} {from_to}");
        }

        break;
    }

    let repo = git2::Repository::open(&repo_path)?;

    type LineLookup = HashMap<PathBuf, Vec<u32>>;

    let mut pr_lines_lookup = HashMap::<u32, LineLookup>::new();

    for pr in data.iter() {
        let branch_head_oid = git2::Oid::from_str(&pr.head_ref_oid)?;
        let branch_head_commit = repo.find_commit(branch_head_oid)?;
        let branch_head_tree = branch_head_commit.tree()?;

        let base_branch = repo.find_branch(&pr.base_ref_name, git2::BranchType::Local)?;
        let base_branch_oid = base_branch.get().peel_to_commit()?.id();

        let common_ancester_oid = repo.merge_base(base_branch_oid, branch_head_oid)?;
        let common_ancestor_commit = repo.find_commit(common_ancester_oid)?;
        let common_ancester_tree = common_ancestor_commit.tree()?;

        let diff =
            repo.diff_tree_to_tree(Some(&common_ancester_tree), Some(&branch_head_tree), None)?;

        let mut file_line_map = LineLookup::new();

        diff.foreach(
            &mut |_, _| true,
            None,
            Some(&mut |diff_delta, diff_hunk| {
                if let Some(path) = diff_delta.old_file().path() {
                    if diff_hunk.old_lines() != 0 {
                        let path = path.to_path_buf();

                        let start = diff_hunk.old_start();
                        let line_count = diff_hunk.old_lines();
                        let mut old_lines: Vec<u32> =
                            (start..(start + line_count)).into_iter().collect();

                        file_line_map
                            .entry(path)
                            .and_modify(|lines| lines.append(&mut old_lines))
                            .or_insert_with(|| old_lines);
                    }
                }
                true
            }),
            None,
        )?;

        pr_lines_lookup.insert(pr.number, file_line_map);
        break;
    }

    println!("{pr_lines_lookup:?}");

    Ok(())
}
