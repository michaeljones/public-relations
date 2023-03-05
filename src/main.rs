///! Runs with a path to a git repo and a json file build with:
///!
///!   gh pr list --limit 500 --json baseRefName --json headRefName --json headRefOid --json headRepository --json headRepositoryOwner --json number --json id > prs.json
///!
///! With problematic entries manually deleted (one with null as the repo due to the fork being deleted)
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::Context;
use serde::Deserialize;
use walkdir::{DirEntry, WalkDir};

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

type LineLookup = HashMap<PathBuf, Vec<u32>>;

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

    let repo = git2::Repository::open(&repo_path)?;

    let json_string = std::fs::read_to_string(&json_path)?;
    let data: Vec<PullRequest> = serde_json::from_str(&json_string)?;

    println!("Fetching pull requests...");

    for pull_request in data.iter().take(100) {
        let user = &pull_request.head_repository_owner.login;
        let repo_name = &pull_request.head_repository.name;
        let from_branch = &pull_request.head_ref_name;
        let to_branch = format!("pull-request-{}", pull_request.number);

        if let Ok(pr_branch) = repo.find_branch(&to_branch, git2::BranchType::Local) {
            let pr_branch_oid = pr_branch
                .get()
                .peel_to_commit()
                .context("Cannot find commit for pull request branch")?
                .id();
            let target_oid = git2::Oid::from_str(&pull_request.head_ref_oid)?;

            // Only run get fetch if the local oid doesn't match the target oid from the json
            if pr_branch_oid != target_oid {
                fetch_pull_request_branch(&repo_path, user, repo_name, from_branch, &to_branch)?;
            }
        } else {
            fetch_pull_request_branch(&repo_path, user, repo_name, from_branch, &to_branch)?;
        }
    }

    println!("Calculating diffs...");

    let mut pr_lines_lookup = HashMap::<u32, LineLookup>::new();

    // Skip files like Cargo.lock where the conflicts are not meaningful
    let ignore_files = [PathBuf::from("Cargo.lock")];

    for pr in data.iter().take(100) {
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

                        if ignore_files.contains(&path) {
                            return true;
                        }

                        // Record all the lines from the old side of the diff which means we record context lines as
                        // well. ie. lines that haven't actually changed. This might be ok as it'll give us an idea
                        // when PRs impact code that is very close to each other but we might also want to try to
                        // improve it in the future
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
    }

    let html = generate_html(&repo_path, &pr_lines_lookup)?;
    std::fs::write("prmap.html", html)?;

    Ok(())
}

fn is_hidden(entry: &DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| s.starts_with("."))
        .unwrap_or(false)
}

fn generate_html(
    repo_path: &Path,
    pr_lines_lookup: &HashMap<u32, LineLookup>,
) -> anyhow::Result<String> {
    let walker = WalkDir::new(repo_path).sort_by_file_name().into_iter();
    let walker = walker
        .filter_entry(|e| !is_hidden(e))
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            if entry.file_type().is_dir() {
                None
            } else {
                entry
                    .path()
                    .strip_prefix(repo_path)
                    .ok()
                    .map(|path| path.to_path_buf())
            }
        });

    let markup = maud::html! {
        (maud::DOCTYPE)
        html {
            head {
                meta charset="utf-8";
                title { }
                style {
                    (maud::PreEscaped(r#"
html {
    font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, "Noto Sans", sans-serif, "Apple Color Emoji", "Segoe UI Emoji", "Segoe UI Symbol", "Noto Color Emoji";

}
                    "#))
                }
            }
            body {
                h1 { "Public Relations" }
                ul {
                    @for entry in walker {
                        (file_list_entry(&entry, pr_lines_lookup))
                    }
                }
            }
        }
    };

    Ok(markup.into_string())
}

fn file_list_entry(entry: &Path, pr_lines_lookup: &HashMap<u32, LineLookup>) -> maud::Markup {
    let total_pr_count = pr_lines_lookup.len();

    let in_pr_count: u32 = pr_lines_lookup
        .iter()
        .map(|(_, value)| value.contains_key(entry) as u32)
        .sum();

    let fraction: f64 = 1.0 - (in_pr_count as f64 / total_pr_count as f64);
    let gradient = colorgrad::spectral();
    let (min, max) = gradient.domain();
    let color = gradient.at(min + fraction * (max - min));
    let style = format!(
        "width: 4rem; height: 1rem; background-color: rgb({}, {}, {})",
        color.r * 255.0,
        color.g * 255.0,
        color.b * 255.0
    );

    let li_style = "display: flex; gap: 1rem;";

    maud::html! {
        li style=(li_style) {
            div style=(style) {
            }
            (entry.display().to_string())
        }
    }
}

fn fetch_pull_request_branch(
    repo_path: &PathBuf,
    user: &str,
    repo_name: &str,
    from_branch: &str,
    to_branch: &str,
) -> anyhow::Result<()> {
    let origin = format!("git@github.com:{user}/{repo_name}");
    let from_to = format!("{from_branch}:{to_branch}");

    println!("Running git fetch {origin} {from_to}");
    let output = std::process::Command::new("git")
        .args(["fetch", &origin, &from_to])
        .current_dir(repo_path)
        .output()?;

    if !output.status.success() {
        anyhow::bail!("Failed to run git fetch for {origin} {from_to}");
    }

    Ok(())
}
