use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::Workspace;
use crate::fetch;
use crate::git;
use crate::run::{self, SummaryValue, compute_summaries};

// ── State types ──────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct Baseline {
    pub commit: String,
    pub timestamp: String,
    pub summaries: HashMap<String, f64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BisectionState {
    pub experiment: String,
    pub summary_name: String,
    pub good: String,
    pub bad: String,
    pub good_value: f64,
    pub bad_value: f64,
    pub threshold: f64,
    pub started_at: String,
}

// ── Report types (stdout JSON) ───────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct StandaloneReport {
    pub results: Vec<ExperimentResult>,
}

#[derive(Debug, Serialize)]
pub struct ExperimentResult {
    pub experiment: String,
    pub action: Action,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Details>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    None,
    BaselineCreated,
    BaselineUpdated,
    RegressionDetected,
    BisectStep,
    CulpritFound,
}

#[derive(Debug, Serialize)]
pub struct Details {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub good: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bad: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub culprit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub culprit_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub culprit_author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regressed_value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regression_pct: Option<f64>,
}

// ── Git operations on data branch ────────────────────────────────────────────

struct DataBranch<'a> {
    repo_dir: &'a Path,
    branch: &'a str,
}

impl<'a> DataBranch<'a> {
    fn new(repo_dir: &'a Path, branch: &'a str) -> Self {
        Self { repo_dir, branch }
    }

    /// Read a file from the data branch without checking it out.
    fn read_file(&self, path: &str) -> Result<Option<String>> {
        let blob = format!("origin/{}:{}", self.branch, path);
        let out = std::process::Command::new("git")
            .args(["show", &blob])
            .current_dir(self.repo_dir)
            .stderr(std::process::Stdio::null())
            .output()
            .context("git show")?;
        if !out.status.success() {
            return Ok(None);
        }
        Ok(Some(String::from_utf8_lossy(&out.stdout).to_string()))
    }

    /// List files under a directory on the data branch.
    fn list_dir(&self, dir: &str) -> Result<Vec<String>> {
        let tree = format!("origin/{}:{}", self.branch, dir);
        let out = std::process::Command::new("git")
            .args(["ls-tree", "--name-only", &tree])
            .current_dir(self.repo_dir)
            .stderr(std::process::Stdio::null())
            .output()
            .context("git ls-tree")?;
        if !out.status.success() {
            return Ok(vec![]);
        }
        Ok(String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|s| s.to_string())
            .collect())
    }

    fn read_baseline(&self, experiment: &str) -> Result<Option<Baseline>> {
        let path = format!("baselines/{experiment}.json");
        match self.read_file(&path)? {
            Some(raw) => Ok(Some(serde_json::from_str(&raw)?)),
            None => Ok(None),
        }
    }

    fn read_active_bisection(&self, experiment: &str) -> Result<Option<BisectionState>> {
        let path = format!("bisection/active/{experiment}.json");
        match self.read_file(&path)? {
            Some(raw) => Ok(Some(serde_json::from_str(&raw)?)),
            None => Ok(None),
        }
    }

    fn list_active_bisections(&self) -> Result<Vec<String>> {
        let files = self.list_dir("bisection/active")?;
        Ok(files
            .into_iter()
            .filter_map(|f| f.strip_suffix(".json").map(|s| s.to_string()))
            .collect())
    }

    /// Write a file to the data branch via a detached commit.
    /// Uses git hash-object + update-ref to avoid checking out the branch.
    fn write_file(&self, path: &str, content: &str, message: &str) -> Result<()> {
        // Ensure the remote branch exists; if not, create an orphan root.
        let branch_exists = std::process::Command::new("git")
            .args(["rev-parse", "--verify", &format!("origin/{}", self.branch)])
            .current_dir(self.repo_dir)
            .stderr(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if !branch_exists {
            self.create_orphan_branch(path, content, message)?;
            return Ok(());
        }

        // Read the current tree of the data branch.
        let parent_ref = format!("origin/{}", self.branch);
        let parent_sha = cmd_output(self.repo_dir, &["rev-parse", &parent_ref])?;

        // Hash the new blob.
        let blob_sha = cmd_stdin_output(self.repo_dir, &["hash-object", "-w", "--stdin"], content)?;

        // Read current tree, update/add the entry, write new tree.
        let tree_sha = self.build_tree_with_file(&parent_sha, path, &blob_sha)?;

        // Create commit.
        let commit_sha = cmd_output(
            self.repo_dir,
            &["commit-tree", &tree_sha, "-p", &parent_sha, "-m", message],
        )?;

        // Push and update local tracking ref.
        let refspec = format!("{commit_sha}:refs/heads/{}", self.branch);
        let status = std::process::Command::new("git")
            .args(["push", "origin", &refspec])
            .current_dir(self.repo_dir)
            .status()
            .context("git push")?;
        if !status.success() {
            bail!("git push to {} failed", self.branch);
        }
        // Update local remote-tracking ref so subsequent reads see this commit.
        let _ = cmd_output(
            self.repo_dir,
            &[
                "update-ref",
                &format!("refs/remotes/origin/{}", self.branch),
                &commit_sha,
            ],
        );

        Ok(())
    }

    /// Remove a file from the data branch.
    fn remove_file(&self, path: &str, message: &str) -> Result<()> {
        let parent_ref = format!("origin/{}", self.branch);
        let parent_sha = cmd_output(self.repo_dir, &["rev-parse", &parent_ref])?;

        // Read the full tree, remove the target path.
        let tree_sha = self.build_tree_without_file(&parent_sha, path)?;

        let commit_sha = cmd_output(
            self.repo_dir,
            &["commit-tree", &tree_sha, "-p", &parent_sha, "-m", message],
        )?;

        let refspec = format!("{commit_sha}:refs/heads/{}", self.branch);
        let status = std::process::Command::new("git")
            .args(["push", "origin", &refspec])
            .current_dir(self.repo_dir)
            .status()
            .context("git push")?;
        if !status.success() {
            bail!("git push to {} failed", self.branch);
        }
        let _ = cmd_output(
            self.repo_dir,
            &[
                "update-ref",
                &format!("refs/remotes/origin/{}", self.branch),
                &commit_sha,
            ],
        );

        Ok(())
    }

    fn create_orphan_branch(&self, path: &str, content: &str, message: &str) -> Result<()> {
        let blob_sha = cmd_stdin_output(self.repo_dir, &["hash-object", "-w", "--stdin"], content)?;

        // Build a tree with just this one file.
        let tree_entry = self.mktree_single(path, &blob_sha)?;

        let commit_sha = cmd_output(self.repo_dir, &["commit-tree", &tree_entry, "-m", message])?;

        let refspec = format!("{commit_sha}:refs/heads/{}", self.branch);
        let status = std::process::Command::new("git")
            .args(["push", "origin", &refspec])
            .current_dir(self.repo_dir)
            .status()
            .context("git push")?;
        if !status.success() {
            bail!("git push to {} failed", self.branch);
        }

        // Fetch so subsequent reads see the new branch.
        let _ = std::process::Command::new("git")
            .args(["fetch", "origin", self.branch])
            .current_dir(self.repo_dir)
            .status();

        Ok(())
    }

    /// Build a tree that adds/replaces `path` (which may contain `/`) with the
    /// given blob, keeping everything else from `parent_sha`.
    /// Build a tree that removes `path` (which may contain `/`) from the
    /// tree of `parent_sha`, keeping everything else.
    fn build_tree_without_file(&self, parent_sha: &str, path: &str) -> Result<String> {
        let parts: Vec<&str> = path.split('/').collect();
        self.remove_from_tree(&format!("{parent_sha}^{{tree}}"), &parts)
    }

    fn remove_from_tree(&self, tree_ref: &str, path_parts: &[&str]) -> Result<String> {
        let mut entries = self.read_tree_entries(tree_ref)?;

        if path_parts.len() == 1 {
            entries.retain(|e| e.name != path_parts[0]);
            return self.write_tree(&entries);
        }

        let dir_name = path_parts[0];
        let sub_entry = entries
            .iter()
            .find(|e| e.name == dir_name && e.kind == "tree");
        if let Some(entry) = sub_entry {
            let new_sub = self.remove_from_tree(&entry.sha, &path_parts[1..])?;
            let sub_entries = self.read_tree_entries(&new_sub)?;
            entries.retain(|e| e.name != dir_name);
            if !sub_entries.is_empty() {
                entries.push(TreeEntry {
                    mode: "040000".to_string(),
                    kind: "tree".to_string(),
                    sha: new_sub,
                    name: dir_name.to_string(),
                });
            }
        }
        self.write_tree(&entries)
    }

    fn build_tree_with_file(&self, parent_sha: &str, path: &str, blob_sha: &str) -> Result<String> {
        // Split path into directory components and filename.
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() == 1 {
            // Simple case: file at root of tree.
            let mut entries = self.read_tree_entries(&format!("{parent_sha}^{{tree}}"))?;
            entries.retain(|e| e.name != parts[0]);
            entries.push(TreeEntry {
                mode: "100644".to_string(),
                kind: "blob".to_string(),
                sha: blob_sha.to_string(),
                name: parts[0].to_string(),
            });
            return self.write_tree(&entries);
        }

        // Nested path: we need to recurse into subtrees.
        self.build_nested_tree(parent_sha, &parts, blob_sha)
    }

    fn build_nested_tree(
        &self,
        parent_sha: &str,
        path_parts: &[&str],
        blob_sha: &str,
    ) -> Result<String> {
        let tree_ref = format!("{parent_sha}^{{tree}}");
        let mut entries = self.read_tree_entries(&tree_ref)?;

        if path_parts.len() == 1 {
            // Leaf: add/replace the blob.
            let name = path_parts[0];
            entries.retain(|e| e.name != name);
            entries.push(TreeEntry {
                mode: "100644".to_string(),
                kind: "blob".to_string(),
                sha: blob_sha.to_string(),
                name: name.to_string(),
            });
            return self.write_tree(&entries);
        }

        // Find or create the subtree for the first component.
        let dir_name = path_parts[0];
        let existing_subtree = entries
            .iter()
            .find(|e| e.name == dir_name && e.kind == "tree");

        let sub_tree_sha = if let Some(entry) = existing_subtree {
            // Recurse into the existing subtree.
            let sub_commit_like = &entry.sha;
            self.build_nested_tree_from_tree(sub_commit_like, &path_parts[1..], blob_sha)?
        } else {
            // No existing subtree — create one from scratch.
            self.build_new_subtree(&path_parts[1..], blob_sha)?
        };

        entries.retain(|e| e.name != dir_name);
        entries.push(TreeEntry {
            mode: "040000".to_string(),
            kind: "tree".to_string(),
            sha: sub_tree_sha,
            name: dir_name.to_string(),
        });
        self.write_tree(&entries)
    }

    fn build_nested_tree_from_tree(
        &self,
        tree_sha: &str,
        path_parts: &[&str],
        blob_sha: &str,
    ) -> Result<String> {
        let mut entries = self.read_tree_entries(tree_sha)?;

        if path_parts.len() == 1 {
            let name = path_parts[0];
            entries.retain(|e| e.name != name);
            entries.push(TreeEntry {
                mode: "100644".to_string(),
                kind: "blob".to_string(),
                sha: blob_sha.to_string(),
                name: name.to_string(),
            });
            return self.write_tree(&entries);
        }

        let dir_name = path_parts[0];
        let existing = entries
            .iter()
            .find(|e| e.name == dir_name && e.kind == "tree");
        let sub_sha = if let Some(entry) = existing {
            self.build_nested_tree_from_tree(&entry.sha, &path_parts[1..], blob_sha)?
        } else {
            self.build_new_subtree(&path_parts[1..], blob_sha)?
        };

        entries.retain(|e| e.name != dir_name);
        entries.push(TreeEntry {
            mode: "040000".to_string(),
            kind: "tree".to_string(),
            sha: sub_sha,
            name: dir_name.to_string(),
        });
        self.write_tree(&entries)
    }

    fn build_new_subtree(&self, path_parts: &[&str], blob_sha: &str) -> Result<String> {
        if path_parts.len() == 1 {
            let entries = vec![TreeEntry {
                mode: "100644".to_string(),
                kind: "blob".to_string(),
                sha: blob_sha.to_string(),
                name: path_parts[0].to_string(),
            }];
            return self.write_tree(&entries);
        }

        let sub_sha = self.build_new_subtree(&path_parts[1..], blob_sha)?;
        let entries = vec![TreeEntry {
            mode: "040000".to_string(),
            kind: "tree".to_string(),
            sha: sub_sha,
            name: path_parts[0].to_string(),
        }];
        self.write_tree(&entries)
    }

    fn mktree_single(&self, path: &str, blob_sha: &str) -> Result<String> {
        let parts: Vec<&str> = path.split('/').collect();
        self.build_new_subtree(&parts, blob_sha)
    }

    fn read_tree_entries(&self, tree_ref: &str) -> Result<Vec<TreeEntry>> {
        let out = std::process::Command::new("git")
            .args(["ls-tree", tree_ref])
            .current_dir(self.repo_dir)
            .stderr(std::process::Stdio::null())
            .output()
            .context("git ls-tree")?;
        if !out.status.success() {
            return Ok(vec![]);
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let mut entries = Vec::new();
        for line in text.lines() {
            // format: <mode> <type> <sha>\t<name>
            let (meta, name) = line.split_once('\t').unwrap_or(("", ""));
            let parts: Vec<&str> = meta.split_whitespace().collect();
            if parts.len() == 3 {
                entries.push(TreeEntry {
                    mode: parts[0].to_string(),
                    kind: parts[1].to_string(),
                    sha: parts[2].to_string(),
                    name: name.to_string(),
                });
            }
        }
        Ok(entries)
    }

    fn write_tree(&self, entries: &[TreeEntry]) -> Result<String> {
        if entries.is_empty() {
            // Empty tree — use git's well-known empty tree hash.
            return cmd_stdin_output(self.repo_dir, &["mktree"], "");
        }
        let input: String = entries
            .iter()
            .map(|e| format!("{} {} {}\t{}", e.mode, e.kind, e.sha, e.name))
            .collect::<Vec<_>>()
            .join("\n");
        cmd_stdin_output(self.repo_dir, &["mktree"], &format!("{input}\n"))
    }
}

struct TreeEntry {
    mode: String,
    kind: String,
    sha: String,
    name: String,
}

fn cmd_output(dir: &Path, args: &[&str]) -> Result<String> {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .stderr(std::process::Stdio::null())
        .output()
        .with_context(|| format!("running git {}", args.join(" ")))?;
    if !out.status.success() {
        bail!("git {} failed", args.join(" "));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn cmd_stdin_output(dir: &Path, args: &[&str], stdin: &str) -> Result<String> {
    use std::io::Write;
    let mut child = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .with_context(|| format!("spawning git {}", args.join(" ")))?;
    child.stdin.take().unwrap().write_all(stdin.as_bytes())?;
    let out = child.wait_with_output()?;
    if !out.status.success() {
        bail!("git {} failed", args.join(" "));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn now_rfc3339() -> String {
    std::process::Command::new("date")
        .arg("+%Y-%m-%dT%H:%M:%S%z")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

// ── Main entry point ─────────────────────────────────────────────────────────

pub fn run_standalone(
    workspace: &Workspace,
    data_branch: &str,
    target_branch: &str,
    threshold: f64,
    mut fetcher: Option<&mut (dyn fetch::PluginFetcher + '_)>,
) -> Result<StandaloneReport> {
    // Fetch latest state from remote.
    git::fetch(&workspace.project_dir)?;

    let db = DataBranch::new(&workspace.project_dir, data_branch);
    let mut results = Vec::new();

    // Check for active bisections first.
    let active_bisections = db.list_active_bisections()?;
    if !active_bisections.is_empty() {
        for experiment_name in &active_bisections {
            let result =
                run_bisection_step(workspace, &db, experiment_name, fetcher.as_deref_mut())?;
            results.push(result);
        }
        return Ok(StandaloneReport { results });
    }

    // No active bisections — run all experiments against HEAD of target branch.
    let target_ref = format!("origin/{target_branch}");
    git::checkout_detached(&workspace.project_dir, &target_ref)
        .with_context(|| format!("checking out {target_ref}"))?;

    let experiments = list_experiment_names(&workspace.project_dir)?;
    if experiments.is_empty() {
        log::info!("no experiments found");
        return Ok(StandaloneReport { results });
    }

    for experiment_name in &experiments {
        let result = run_experiment_and_compare(
            workspace,
            &db,
            experiment_name,
            threshold,
            fetcher.as_deref_mut(),
        )?;
        results.push(result);
        // Reset worktree between experiments (patches may have been applied).
        git::reset_worktree(&workspace.project_dir)?;
    }

    Ok(StandaloneReport { results })
}

fn list_experiment_names(repo_dir: &std::path::Path) -> Result<Vec<String>> {
    let experiments_dir = repo_dir.join(".wezel").join("experiments");
    if !experiments_dir.is_dir() {
        return Ok(vec![]);
    }
    let mut names = Vec::new();
    for entry in std::fs::read_dir(&experiments_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir()
            && path.join("experiment.toml").is_file()
            && let Some(name) = path.file_name().and_then(|n| n.to_str())
        {
            names.push(name.to_string());
        }
    }
    names.sort();
    Ok(names)
}

fn run_experiment_and_compare(
    workspace: &Workspace,
    db: &DataBranch,
    experiment_name: &str,
    threshold: f64,
    fetcher: Option<&mut (dyn fetch::PluginFetcher + '_)>,
) -> Result<ExperimentResult> {
    log::info!("running experiment: {experiment_name}");

    let (step_reports, summary_defs) = run::run_experiment(experiment_name, workspace, fetcher)?;
    let computed = compute_summaries(&step_reports, &summary_defs);
    let commit = git::current_sha(&workspace.project_dir)?;

    // Read existing baseline.
    let existing_baseline = db.read_baseline(experiment_name)?;

    match existing_baseline {
        None => {
            // First run — create baseline.
            let baseline = Baseline {
                commit: commit.clone(),
                timestamp: now_rfc3339(),
                summaries: computed.iter().map(|(k, v)| (k.clone(), v.value)).collect(),
            };
            let json = serde_json::to_string_pretty(&baseline)?;
            db.write_file(
                &format!("baselines/{experiment_name}.json"),
                &json,
                &format!(
                    "baseline: {experiment_name} @ {}",
                    &commit[..7.min(commit.len())]
                ),
            )?;
            Ok(ExperimentResult {
                experiment: experiment_name.to_string(),
                action: Action::BaselineCreated,
                details: None,
            })
        }
        Some(baseline) => {
            // Compare each bisect-eligible summary against baseline.
            if let Some(regression) = detect_regression(&baseline, &computed, threshold) {
                // Start bisection.
                let state = BisectionState {
                    experiment: experiment_name.to_string(),
                    summary_name: regression.summary_name.clone(),
                    good: baseline.commit.clone(),
                    bad: commit.clone(),
                    good_value: regression.baseline_value,
                    bad_value: regression.current_value,
                    threshold,
                    started_at: now_rfc3339(),
                };
                let json = serde_json::to_string_pretty(&state)?;
                db.write_file(
                    &format!("bisection/active/{experiment_name}.json"),
                    &json,
                    &format!(
                        "bisect: {experiment_name}/{} regressed {:.1}%",
                        regression.summary_name, regression.regression_pct
                    ),
                )?;
                Ok(ExperimentResult {
                    experiment: experiment_name.to_string(),
                    action: Action::RegressionDetected,
                    details: Some(Details {
                        summary_name: Some(regression.summary_name),
                        good: Some(baseline.commit),
                        bad: Some(commit),
                        baseline_value: Some(regression.baseline_value),
                        regressed_value: Some(regression.current_value),
                        regression_pct: Some(regression.regression_pct),
                        ..Details::empty()
                    }),
                })
            } else {
                // No regression — update baseline.
                let new_baseline = Baseline {
                    commit: commit.clone(),
                    timestamp: now_rfc3339(),
                    summaries: computed.iter().map(|(k, v)| (k.clone(), v.value)).collect(),
                };
                let json = serde_json::to_string_pretty(&new_baseline)?;
                db.write_file(
                    &format!("baselines/{experiment_name}.json"),
                    &json,
                    &format!(
                        "update: {experiment_name} @ {}",
                        &commit[..7.min(commit.len())]
                    ),
                )?;
                Ok(ExperimentResult {
                    experiment: experiment_name.to_string(),
                    action: Action::BaselineUpdated,
                    details: None,
                })
            }
        }
    }
}

struct Regression {
    summary_name: String,
    baseline_value: f64,
    current_value: f64,
    regression_pct: f64,
}

fn detect_regression(
    baseline: &Baseline,
    current: &HashMap<String, SummaryValue>,
    threshold: f64,
) -> Option<Regression> {
    // Return the first bisect-eligible summary that exceeds the threshold.
    for (name, sv) in current {
        if !sv.bisect {
            continue;
        }
        let Some(&baseline_value) = baseline.summaries.get(name) else {
            continue;
        };
        if baseline_value == 0.0 {
            continue;
        }
        let pct = ((sv.value - baseline_value) / baseline_value) * 100.0;
        if pct > threshold {
            return Some(Regression {
                summary_name: name.clone(),
                baseline_value,
                current_value: sv.value,
                regression_pct: pct,
            });
        }
    }
    None
}

fn run_bisection_step(
    workspace: &Workspace,
    db: &DataBranch,
    experiment_name: &str,
    fetcher: Option<&mut (dyn fetch::PluginFetcher + '_)>,
) -> Result<ExperimentResult> {
    let state = db
        .read_active_bisection(experiment_name)?
        .with_context(|| format!("reading bisection state for {experiment_name}"))?;

    // List commits in good..bad range.
    let range = format!("{}..{}", state.good, state.bad);
    let commits_output = cmd_output(&workspace.project_dir, &["rev-list", "--reverse", &range])?;
    let commits: Vec<&str> = commits_output.lines().collect();

    if commits.len() <= 1 {
        // The bad commit IS the culprit.
        let culprit = &state.bad;
        git::checkout_detached(&workspace.project_dir, culprit)?;
        let message = git::commit_message(&workspace.project_dir);
        let author = git::commit_author(&workspace.project_dir);
        let pct = if state.good_value != 0.0 {
            ((state.bad_value - state.good_value) / state.good_value) * 100.0
        } else {
            0.0
        };

        // Move bisection state to completed.
        let completed_path = format!(
            "bisection/completed/{}-{}.json",
            experiment_name,
            now_rfc3339().replace(':', "-")
        );
        let completed_json = serde_json::to_string_pretty(&state)?;
        db.write_file(
            &completed_path,
            &completed_json,
            &format!(
                "bisect complete: {experiment_name} culprit {}",
                &culprit[..7.min(culprit.len())]
            ),
        )?;
        db.remove_file(
            &format!("bisection/active/{experiment_name}.json"),
            &format!("bisect cleanup: {experiment_name}"),
        )?;

        return Ok(ExperimentResult {
            experiment: experiment_name.to_string(),
            action: Action::CulpritFound,
            details: Some(Details {
                summary_name: Some(state.summary_name),
                culprit: Some(culprit.to_string()),
                culprit_message: Some(message),
                culprit_author: Some(author),
                baseline_value: Some(state.good_value),
                regressed_value: Some(state.bad_value),
                regression_pct: Some(pct),
                good: Some(state.good.clone()),
                bad: Some(state.bad.clone()),
            }),
        });
    }

    // Pick the midpoint.
    let mid_idx = commits.len() / 2;
    let midpoint = commits[mid_idx];
    log::info!(
        "bisecting {experiment_name}: testing {} ({}/{})",
        &midpoint[..7.min(midpoint.len())],
        mid_idx + 1,
        commits.len()
    );

    git::checkout_detached(&workspace.project_dir, midpoint)?;
    let (step_reports, summary_defs) = run::run_experiment(experiment_name, workspace, fetcher)?;
    let computed = compute_summaries(&step_reports, &summary_defs);

    // Compare midpoint value against known-good.
    let mid_value = computed
        .get(&state.summary_name)
        .map(|v| v.value)
        .unwrap_or(0.0);

    let pct = if state.good_value != 0.0 {
        ((mid_value - state.good_value) / state.good_value) * 100.0
    } else {
        0.0
    };

    let (new_good, new_bad) = if pct > state.threshold {
        // Midpoint is regressed — culprit is in good..midpoint.
        (state.good.clone(), midpoint.to_string())
    } else {
        // Midpoint is fine — culprit is in midpoint..bad.
        (midpoint.to_string(), state.bad.clone())
    };

    let new_state = BisectionState {
        good: new_good.clone(),
        bad: new_bad.clone(),
        ..state
    };
    let json = serde_json::to_string_pretty(&new_state)?;
    db.write_file(
        &format!("bisection/active/{experiment_name}.json"),
        &json,
        &format!(
            "bisect: {experiment_name} narrowed to {}..{}",
            &new_good[..7.min(new_good.len())],
            &new_bad[..7.min(new_bad.len())]
        ),
    )?;

    // Reset worktree after bisection step.
    git::reset_worktree(&workspace.project_dir)?;

    Ok(ExperimentResult {
        experiment: experiment_name.to_string(),
        action: Action::BisectStep,
        details: Some(Details {
            summary_name: Some(new_state.summary_name),
            good: Some(new_good),
            bad: Some(new_bad),
            ..Details::empty()
        }),
    })
}

impl Details {
    fn empty() -> Self {
        Self {
            summary_name: None,
            good: None,
            bad: None,
            culprit: None,
            culprit_message: None,
            culprit_author: None,
            baseline_value: None,
            regressed_value: None,
            regression_pct: None,
        }
    }
}
