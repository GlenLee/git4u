//! g4u-core — Git 引擎适配层。
//!
//! M0 先落地三引擎中的 **系统 git CLI** 实现（ADR-2 的兜底路径）：语义 100% 与命令行
//! 一致，且能覆盖 hooks / credential / config 等长尾。gitoxide（读）与 git2（写）的
//! 实现后续在同一 `GitRead` / `GitWrite` trait 后替换，ops 层与前端零改动。
//!
//! shortcut: M0 只实现 CLI 引擎；读路径性能达标（gix）在 M0 性能门之后接入。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use g4u_model::{
    suggest, BackupRef, BranchInfo, BranchListing, ChangeKind, CommitDetail, CommitSummary,
    DiffHunk, DiffLine, DiffLineKind, EolFile, EolReport, ErrorKind, FileDiff, FileStat, G4uError,
    HeadKind, HeadState, IgnoreMatch, ObjectStats, Oid, PullStrategy, ReflogEntry, RemoteInfo,
    RepoStats, RepoStatus, Risk, StatusEntry, TagInfo, UpstreamInfo,
};

// ---------------------------------------------------------------------------
// Trait 边界（03-9）
// ---------------------------------------------------------------------------

pub trait GitRead: Send + Sync {
    fn status(&self) -> Result<RepoStatus, G4uError>;
    fn log(&self, limit: usize) -> Result<Vec<CommitSummary>, G4uError>;
    fn branches(&self) -> Result<BranchListing, G4uError>;
    fn diff(&self, path: &str, staged: bool) -> Result<FileDiff, G4uError>;
    fn commit_detail(&self, oid: &str) -> Result<CommitDetail, G4uError>;
}

pub trait GitWrite: Send + Sync {
    fn stage(&self, paths: &[String]) -> Result<(), G4uError>;
    fn unstage(&self, paths: &[String]) -> Result<(), G4uError>;
    fn commit(&self, message: &str, amend: bool, run_hooks: bool) -> Result<Oid, G4uError>;
    fn checkout_branch(&self, name: &str) -> Result<(), G4uError>;
    fn create_branch(
        &self,
        name: &str,
        start: Option<&str>,
        checkout: bool,
    ) -> Result<(), G4uError>;
    fn fetch(
        &self,
        remote: Option<&str>,
        credential: Option<&AskpassCredential>,
        proxy: Option<&str>,
    ) -> Result<String, G4uError>;
    fn pull(
        &self,
        strategy: PullStrategy,
        credential: Option<&AskpassCredential>,
        proxy: Option<&str>,
    ) -> Result<(), G4uError>;
    fn push(
        &self,
        remote: &str,
        branch: &str,
        set_upstream: bool,
        credential: Option<&AskpassCredential>,
        proxy: Option<&str>,
    ) -> Result<(), G4uError>;
}

/// 通过 askpass 注入的一次性凭据；不写入 git 配置或磁盘。
#[derive(Debug, Clone)]
pub struct AskpassCredential {
    pub username: String,
    pub password: String,
}

// ---------------------------------------------------------------------------
// SystemGit
// ---------------------------------------------------------------------------

/// 系统 git 版本（用于设置页「关于」；不依赖仓库）。
pub fn git_version() -> String {
    Command::new("git")
        .arg("--version")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            text.strip_prefix("git version ")
                .unwrap_or(&text)
                .to_string()
        })
        .unwrap_or_else(|| "未找到 git".to_string())
}

#[derive(Debug, Clone)]
pub struct SystemGit {
    /// 工作区根（bare 仓库时为 git dir）。
    pub workdir: PathBuf,
    pub git_dir: PathBuf,
    pub is_bare: bool,
}

impl SystemGit {
    /// 从任意目录（含子目录）定位仓库。
    pub fn discover(path: &Path) -> Result<Self, G4uError> {
        if !path.is_dir() {
            return Err(not_a_repo(path, "目录不存在或不是文件夹"));
        }

        let probe = |args: &[&str]| -> Option<String> {
            Command::new("git")
                .arg("-C")
                .arg(path)
                .args(args)
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        };

        if probe(&["--version"]).is_none() {
            return Err(G4uError::new(
                ErrorKind::GitNotFound,
                "未找到系统 git。Git4U 需要 git ≥ 2.34（用于网络、hooks 与兼容兜底）。",
            )
            .with_suggestions(vec![suggest(
                "安装 Git 后重新打开",
                Risk::Safe,
                "app.retry",
            )]));
        }

        let is_bare = probe(&["rev-parse", "--is-bare-repository"]).as_deref() == Some("true");
        let root = if is_bare {
            probe(&["rev-parse", "--absolute-git-dir"])
        } else {
            probe(&["rev-parse", "--show-toplevel"])
        };
        let Some(root) = root else {
            return Err(not_a_repo(path, "这里不是 Git 仓库（未找到 .git）"));
        };
        let git_dir = probe(&["rev-parse", "--absolute-git-dir"])
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(&root).join(".git"));

        Ok(Self {
            workdir: PathBuf::from(root),
            git_dir,
            is_bare,
        })
    }

    pub fn name(&self) -> String {
        self.workdir
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.workdir.to_string_lossy().into_owned())
    }

    /// 仪表盘统计。任何子项失败都退化为 0/None，不阻断仓库列表。
    pub fn stats(&self) -> RepoStats {
        let refs = self
            .run_opt(&[
                "for-each-ref",
                "--format=%(refname)",
                "refs/heads",
                "refs/tags",
            ])
            .unwrap_or_default();
        let branch_count = refs
            .lines()
            .filter(|line| line.starts_with("refs/heads/"))
            .count();
        let tag_count = refs
            .lines()
            .filter(|line| line.starts_with("refs/tags/"))
            .count();

        let last = self
            .run_opt(&["log", "-1", "--format=%ct%x00%s", "HEAD"])
            .and_then(|raw| {
                let (at, summary) = raw.split_once('\0')?;
                Some((at.trim().parse::<i64>().ok()?, summary.to_string()))
            });

        RepoStats {
            size_bytes: dir_size(&self.git_dir),
            commit_count: self
                .run_opt(&["rev-list", "--count", "HEAD"])
                .and_then(|raw| raw.trim().parse().ok())
                .unwrap_or(0),
            branch_count,
            tag_count,
            last_commit_at: last.as_ref().map(|(at, _)| *at),
            last_commit_summary: last.map(|(_, summary)| summary),
        }
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(&self.workdir);
        cmd.args(["-c", "core.quotepath=false"]);
        cmd.args(args);
        cmd.env("GIT_TERMINAL_PROMPT", "0");
        #[cfg(not(windows))]
        cmd.env("LC_ALL", "C");
        cmd
    }

    fn execute(&self, args: &[&str], stdin: Option<&str>) -> Result<Output, G4uError> {
        self.execute_with_network(args, stdin, None, None)
    }

    fn execute_with_network(
        &self,
        args: &[&str],
        stdin: Option<&str>,
        credential: Option<&AskpassCredential>,
        proxy: Option<&str>,
    ) -> Result<Output, G4uError> {
        let mut cmd = self.command(args);
        if let Some(credential) = credential {
            if let Ok(exe) = std::env::current_exe() {
                cmd.env("GIT_ASKPASS", exe);
            }
            cmd.env("G4U_ASKPASS", "1");
            cmd.env("G4U_ASKPASS_USER", &credential.username);
            cmd.env("G4U_ASKPASS_PASS", &credential.password);
            // 清空继承的 helper 链，只让 askpass 回答本次请求。
            cmd.env("GIT_CONFIG_COUNT", "1");
            cmd.env("GIT_CONFIG_KEY_0", "credential.helper");
            cmd.env("GIT_CONFIG_VALUE_0", "");
        }
        apply_proxy(&mut cmd, proxy);
        cmd.stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                G4uError::new(ErrorKind::GitNotFound, "未找到系统 git。")
            } else {
                G4uError::internal(format!("无法启动 git: {e}"))
            }
        })?;
        if let Some(input) = stdin {
            if let Some(mut pipe) = child.stdin.take() {
                pipe.write_all(input.as_bytes())
                    .map_err(|e| G4uError::internal(format!("写入 git 标准输入失败: {e}")))?;
            }
        }
        child
            .wait_with_output()
            .map_err(|e| G4uError::internal(format!("等待 git 结束失败: {e}")))
    }

    fn run(&self, args: &[&str]) -> Result<String, G4uError> {
        let out = self.execute(args, None)?;
        if !out.status.success() {
            return Err(self.failure(args, &out));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// 允许 `git` 用退出码 1 表示“没有结果”（如 check-ignore / config --get）。
    fn run_optional(&self, args: &[&str]) -> Result<Option<String>, G4uError> {
        let out = self.execute(args, None)?;
        match out.status.code() {
            Some(0) => Ok(Some(
                String::from_utf8_lossy(&out.stdout).trim().to_string(),
            )),
            Some(1) => Ok(None),
            _ => Err(self.failure(args, &out)),
        }
    }

    fn run_with_stdin(&self, args: &[&str], input: &str) -> Result<String, G4uError> {
        let out = self.execute(args, Some(input))?;
        if !out.status.success() {
            return Err(self.failure(args, &out));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn run_with_credential(
        &self,
        args: &[&str],
        credential: Option<&AskpassCredential>,
        proxy: Option<&str>,
    ) -> Result<String, G4uError> {
        let out = self.execute_with_network(args, None, credential, proxy)?;
        if !out.status.success() {
            return Err(self.failure(args, &out));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// 只读探测远端认证；用于设置页「验证连接」。
    pub fn ls_remote(
        &self,
        url: &str,
        credential: Option<&AskpassCredential>,
        proxy: Option<&str>,
    ) -> Result<String, G4uError> {
        self.run_with_credential(&["ls-remote", "--heads", url], credential, proxy)
    }

    /// 已跟踪 + 未跟踪（排除忽略）的工作区文件，按路径排序。
    pub fn list_files(&self) -> Result<Vec<String>, G4uError> {
        let raw = self.run_bytes(&[
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])?;
        let mut files: Vec<String> = raw
            .split(|byte| *byte == 0)
            .filter(|chunk| !chunk.is_empty())
            .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
            .collect();
        files.sort();
        files.dedup();
        Ok(files)
    }

    /// 解释某个路径由哪条 ignore 规则命中；未命中返回 None。
    pub fn check_ignore(&self, path: &str) -> Result<Option<IgnoreMatch>, G4uError> {
        let Some(line) = self.run_optional(&["check-ignore", "-v", "--", path])? else {
            return Ok(None);
        };
        let Some((meta, _)) = line.split_once('\t') else {
            return Ok(None);
        };
        let mut parts = meta.rsplitn(3, ':');
        let pattern = parts.next().unwrap_or("").to_string();
        let line_no = parts
            .next()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        let source = parts.next().unwrap_or("").to_string();
        Ok(Some(IgnoreMatch {
            source,
            line: line_no,
            pattern,
        }))
    }

    /// 工作区文件的 EOL 概览；只保留 mixed 文件明细（最多 200 条）。
    pub fn eol_report(&self) -> Result<EolReport, G4uError> {
        let raw = self.run(&["ls-files", "--eol"])?;
        let mut report = EolReport {
            total: 0,
            lf: 0,
            crlf: 0,
            mixed: 0,
            files: Vec::new(),
        };
        for line in raw.lines() {
            let Some((meta, path)) = line.rsplit_once('\t') else {
                continue;
            };
            let mut parts = meta.split_whitespace();
            let index = parts
                .next()
                .unwrap_or("")
                .trim_start_matches("i/")
                .to_string();
            let worktree = parts
                .next()
                .unwrap_or("")
                .trim_start_matches("w/")
                .to_string();
            let attr = parts
                .next()
                .unwrap_or("")
                .trim_start_matches("attr/")
                .to_string();
            report.total += 1;
            let mixed = index == "mixed" || worktree == "mixed";
            if mixed {
                report.mixed += 1;
                if report.files.len() < 200 {
                    report.files.push(EolFile {
                        path: path.to_string(),
                        index,
                        worktree,
                        attr,
                    });
                }
                continue;
            }
            match worktree.as_str() {
                "lf" => report.lf += 1,
                "crlf" => report.crlf += 1,
                _ => {}
            }
        }
        Ok(report)
    }

    /// `git count-objects -v` 的可视化摘要（size 单位为 KiB）。
    pub fn object_stats(&self) -> Result<ObjectStats, G4uError> {
        let raw = self.run(&["count-objects", "-v"])?;
        let mut stats = ObjectStats::default();
        for line in raw.lines() {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let number = value.trim().parse::<u64>().unwrap_or(0);
            match key.trim() {
                "count" => stats.count = number,
                "size" => stats.size_kb = number,
                "in-pack" => stats.in_pack = number,
                "size-pack" => stats.size_pack_kb = number,
                "packs" => stats.packs = number,
                "prune-packable" => stats.prune_packable = number,
                "garbage" => stats.garbage = number,
                "size-garbage" => stats.size_garbage_kb = number,
                _ => {}
            }
        }
        Ok(stats)
    }

    /// 所有引用的 reflog（含 HEAD），用于误删/误 reset 的恢复线索。
    pub fn reflog(&self, limit: usize) -> Result<Vec<ReflogEntry>, G4uError> {
        let limit = limit.clamp(1, 500).to_string();
        let raw = self.run(&[
            "reflog",
            "--all",
            "--date=unix",
            "--format=%H%x00%gd%x00%gs%x00%ct",
            "-n",
            &limit,
        ])?;
        let mut entries = Vec::new();
        for line in raw.lines() {
            let fields: Vec<&str> = line.split('\0').collect();
            if fields.len() < 4 {
                continue;
            }
            entries.push(ReflogEntry {
                oid: fields[0].to_string(),
                selector: fields[1].to_string(),
                subject: fields[2].to_string(),
                at: fields[3].parse().unwrap_or(0),
            });
        }
        Ok(entries)
    }

    pub fn config_get(&self, key: &str) -> Result<Option<String>, G4uError> {
        self.run_optional(&["config", "--get", key])
    }

    /// 只做连通性检查，输出警告/错误文本；不修复、不删除对象。
    pub fn fsck(&self) -> Result<String, G4uError> {
        let out = self.execute(&["fsck", "--connectivity-only", "--no-progress"], None)?;
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Ok(match (stdout.is_empty(), stderr.is_empty()) {
            (false, false) => format!("{stdout}\n{stderr}"),
            (false, true) => stdout,
            (true, false) => stderr,
            (true, true) => String::new(),
        })
    }

    /// `refs/git4u/backup/*` 下的恢复点，按引用名（时间戳）排序。
    pub fn backup_refs(&self) -> Result<Vec<BackupRef>, G4uError> {
        let raw = self.run(&[
            "for-each-ref",
            "--format=%(refname)%00%(objectname)",
            "refs/git4u/backup/",
        ])?;
        let mut refs = Vec::new();
        for line in raw.lines() {
            let mut parts = line.split('\0');
            let (Some(name), Some(oid)) = (parts.next(), parts.next()) else {
                continue;
            };
            if name.is_empty() || oid.is_empty() {
                continue;
            }
            refs.push(BackupRef {
                name: name.to_string(),
                oid: oid.to_string(),
            });
        }
        Ok(refs)
    }

    /// 清理超过保留期的备份引用；`days = 0` 时永不清理。
    pub fn cleanup_backup_refs(&self, days: u32) -> Result<usize, G4uError> {
        if days == 0 {
            return Ok(0);
        }
        let cutoff = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0)
            .saturating_sub(days as u64 * 86_400);
        let raw = self.run(&["for-each-ref", "--format=%(refname)", "refs/git4u/backup/"])?;
        let mut deletes = String::new();
        let mut count = 0;
        for name in raw.lines() {
            let Some(timestamp) = parse_backup_timestamp(name) else {
                continue;
            };
            if timestamp < cutoff {
                deletes.push_str(&format!("delete {name}\n"));
                count += 1;
            }
        }
        if count > 0 {
            self.run_with_stdin(&["update-ref", "--stdin"], &deletes)?;
        }
        Ok(count)
    }

    /// 非零退出是可接受结果的探测（如 symbolic-ref 在 detached HEAD 时）。
    fn run_opt(&self, args: &[&str]) -> Option<String> {
        let out = self.execute(args, None).ok()?;
        if !out.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    }

    fn run_bytes(&self, args: &[&str]) -> Result<Vec<u8>, G4uError> {
        let out = self.execute(args, None)?;
        if !out.status.success() {
            return Err(self.failure(args, &out));
        }
        Ok(out.stdout)
    }

    fn failure(&self, args: &[&str], out: &Output) -> G4uError {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let cli = format!(
            "git -C {} {}",
            self.workdir.to_string_lossy(),
            args.join(" ")
        );
        classify_failure(&stderr).with_cli(cli)
    }

    // -----------------------------------------------------------------------
    // 读
    // -----------------------------------------------------------------------

    pub fn head(&self) -> Result<HeadState, G4uError> {
        let branch = self.run_opt(&["symbolic-ref", "--quiet", "--short", "HEAD"]);
        let oid = self.run_opt(&["rev-parse", "--verify", "--quiet", "HEAD"]);
        Ok(match (branch, oid) {
            (Some(branch), Some(oid)) => HeadState {
                kind: HeadKind::Branch,
                branch: Some(branch),
                oid: Some(oid),
            },
            (Some(branch), None) => HeadState {
                kind: HeadKind::Unborn,
                branch: Some(branch),
                oid: None,
            },
            (None, Some(oid)) => HeadState {
                kind: HeadKind::Detached,
                branch: None,
                oid: Some(oid),
            },
            (None, None) => HeadState {
                kind: HeadKind::Unborn,
                branch: None,
                oid: None,
            },
        })
    }

    pub fn upstream(&self, branch: &str) -> Option<UpstreamInfo> {
        let range = format!("{branch}@{{upstream}}");
        let name = self.run_opt(&["rev-parse", "--abbrev-ref", "--symbolic-full-name", &range])?;

        let track = self
            .run_opt(&[
                "for-each-ref",
                "--format=%(upstream:track)",
                &format!("refs/heads/{branch}"),
            ])
            .unwrap_or_default();

        let (ahead, behind, gone) = parse_track(&track);
        Some(UpstreamInfo {
            name,
            ahead,
            behind,
            gone,
        })
    }

    pub fn remotes(&self) -> Result<Vec<RemoteInfo>, G4uError> {
        let raw = self.run(&["remote", "-v"])?;
        let mut remotes: Vec<RemoteInfo> = Vec::new();
        for line in raw.lines() {
            let mut parts = line.split_whitespace();
            let Some(name) = parts.next() else { continue };
            let Some(url) = parts.next() else { continue };
            if parts.next() != Some("(fetch)") {
                continue;
            }
            if !remotes.iter().any(|remote| remote.name == name) {
                remotes.push(RemoteInfo {
                    name: name.to_string(),
                    url: url.to_string(),
                });
            }
        }
        Ok(remotes)
    }

    pub fn range_files(&self, range: &str) -> Result<Vec<String>, G4uError> {
        let raw = self.run_bytes(&["diff", "--name-only", "-z", range, "--"])?;
        Ok(raw
            .split(|byte| *byte == 0)
            .filter(|chunk| !chunk.is_empty())
            .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
            .collect())
    }

    pub fn range_diff(&self, range: &str) -> Result<String, G4uError> {
        self.run(&["diff", "--unified=0", "--no-color", range, "--"])
    }

    /// 首次 push 没有 upstream 时，只扫描最新提交（shortcut: 不遍历全部历史）。
    pub fn head_diff(&self) -> Result<String, G4uError> {
        self.run(&["show", "--format=", "--unified=0", "--no-color", "HEAD"])
    }

    pub fn mid_state(&self) -> Option<String> {
        let has = |name: &str| self.git_dir.join(name).exists();
        if has("MERGE_HEAD") {
            Some("合并中".into())
        } else if has("rebase-merge") || has("rebase-apply") {
            Some("rebase 中".into())
        } else if has("CHERRY_PICK_HEAD") {
            Some("cherry-pick 中".into())
        } else if has("REVERT_HEAD") {
            Some("revert 中".into())
        } else if has("BISECT_LOG") {
            Some("bisect 中".into())
        } else {
            None
        }
    }

    fn numstat(&self, args: &[&str]) -> Result<Vec<(String, u32, u32)>, G4uError> {
        let raw = self.run_bytes(args)?;
        let mut result = Vec::new();
        let chunks: Vec<&[u8]> = raw.split(|b| *b == 0).collect();
        let mut i = 0;
        while i < chunks.len() {
            let chunk = chunks[i];
            i += 1;
            if chunk.is_empty() {
                continue;
            }
            let mut fields = chunk.splitn(3, |b| *b == b'\t');
            let add = fields.next().unwrap_or_default();
            let del = fields.next().unwrap_or_default();
            let path = fields.next().unwrap_or_default();

            let (old, new) = if path.is_empty() {
                // rename: add\tdel\t\0old\0new
                let old = chunks.get(i).copied().unwrap_or_default();
                let new = chunks.get(i + 1).copied().unwrap_or_default();
                i += 2;
                (Some(old), new)
            } else {
                (None, path)
            };
            let _ = old;
            let path = String::from_utf8_lossy(new).into_owned();
            if path.is_empty() {
                continue;
            }
            let add = parse_num(add);
            let del = parse_num(del);
            result.push((path, add, del));
        }
        Ok(result)
    }

    // -----------------------------------------------------------------------
    // 写
    // -----------------------------------------------------------------------

    pub fn rev_parse_head(&self) -> Result<Oid, G4uError> {
        self.run(&["rev-parse", "HEAD"])
            .map(|s| s.trim().to_string())
    }

    /// 写入 `refs/git4u/backup/<UTC 时间>-<label>`，返回引用名。
    pub fn create_backup_ref(&self, label: &str, oid: &str) -> Result<String, G4uError> {
        let label: String = label
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect();
        let label = if label.is_empty() {
            "op".to_string()
        } else {
            label
        };
        let short: String = oid.chars().take(7).collect();
        let name = format!("refs/git4u/backup/{}-{label}-{short}", backup_stamp());
        self.run(&["update-ref", &name, oid])?;
        Ok(name)
    }

    /// 只移动 HEAD / 分支指针，保留暂存区与工作区；用于撤销提交。
    pub fn reset_soft(&self, oid: &str) -> Result<(), G4uError> {
        self.run(&["reset", "--soft", oid])?;
        Ok(())
    }
}

fn backup_stamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    let days = seconds.div_euclid(86_400);
    let rest = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        rest / 3600,
        (rest % 3600) / 60,
        rest % 60
    )
}

/// Howard Hinnant 的 civil_from_days，避免为时间戳引入日期库。
fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_pos = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_pos + 2) / 5 + 1;
    let month = if month_pos < 10 {
        month_pos + 3
    } else {
        month_pos - 9
    };
    (
        if month <= 2 { year + 1 } else { year } as i32,
        month as u32,
        day as u32,
    )
}

fn parse_backup_timestamp(name: &str) -> Option<u64> {
    let leaf = name.strip_prefix("refs/git4u/backup/")?;
    if leaf.len() < 15 || leaf.as_bytes().get(8) != Some(&b'-') {
        return None;
    }
    let year: i32 = leaf[0..4].parse().ok()?;
    let month: u32 = leaf[4..6].parse().ok()?;
    let day: u32 = leaf[6..8].parse().ok()?;
    let hour: u32 = leaf[9..11].parse().ok()?;
    let minute: u32 = leaf[11..13].parse().ok()?;
    let second: u32 = leaf[13..15].parse().ok()?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    let days = days_from_civil(year, month, day);
    if days < 0 {
        return None;
    }
    Some(days as u64 * 86_400 + hour as u64 * 3_600 + minute as u64 * 60 + second as u64)
}

/// Howard Hinnant 的 civil date → days since 1970-01-01。
fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = year - if month <= 2 { 1 } else { 0 };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 } as i64;
    let doy = (153 * mp + 2) / 5 + day as i64 - 1;
    let doe = yoe as i64 * 365 + yoe as i64 / 4 - yoe as i64 / 100 + doy;
    era as i64 * 146_097 + doe - 719_468
}

fn parse_num(raw: &[u8]) -> u32 {
    if raw == b"-" {
        return 0;
    }
    String::from_utf8_lossy(raw).parse().unwrap_or(0)
}

fn parse_track(track: &str) -> (u32, u32, bool) {
    if track.contains("gone") {
        return (0, 0, true);
    }
    let pick = |key: &str| -> u32 {
        track
            .find(key)
            .and_then(|at| {
                track[at + key.len()..]
                    .split(|c: char| !c.is_ascii_digit())
                    .next()
            })
            .and_then(|n| n.parse().ok())
            .unwrap_or(0)
    };
    (pick("ahead "), pick("behind "), false)
}

fn not_a_repo(path: &Path, human: &str) -> G4uError {
    G4uError::new(ErrorKind::NotARepo, human)
        .with_cause(format!("路径：{}", path.display()))
        .with_suggestions(vec![suggest("选择其他目录", Risk::Safe, "repo.pick")])
}

fn apply_proxy(cmd: &mut Command, proxy: Option<&str>) {
    for key in [
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "NO_PROXY",
        "no_proxy",
    ] {
        cmd.env_remove(key);
    }
    match proxy {
        Some(proxy) => {
            for key in ["HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY"] {
                cmd.env(key, proxy);
            }
        }
        // 明确「不使用代理」：阻止系统/环境代理影响本次命令。
        None => {
            cmd.env("NO_PROXY", "*");
        }
    }
}

fn classify_failure(stderr: &str) -> G4uError {
    let lower = stderr.to_lowercase();
    let (kind, human) = if lower.contains("index.lock") {
        (
            ErrorKind::IndexLocked,
            "仓库被另一个 Git 进程锁定（index.lock）。通常几秒后自动消失。",
        )
    } else if lower.contains("not a git repository") {
        (ErrorKind::NotARepo, "这不是一个 Git 仓库。")
    } else if lower.contains("your local changes")
        || lower.contains("would be overwritten")
        || lower.contains("please commit your changes or stash")
    {
        (
            ErrorKind::CheckoutConflict,
            "工作区有未提交改动，切换分支会覆盖它们。请先提交或搁置改动。",
        )
    } else if lower.contains("please tell me who you are")
        || lower.contains("unable to auto-detect email")
    {
        (
            ErrorKind::IdentityMissing,
            "Git 还没有配置提交者身份（user.name / user.email）。",
        )
    } else if lower.contains("authentication failed")
        || lower.contains("could not read username")
        || lower.contains("permission denied")
    {
        (
            ErrorKind::AuthFailed,
            "远端认证失败，请检查凭证或 SSH 密钥。",
        )
    } else if lower.contains("nothing to commit") || lower.contains("no changes added to commit") {
        (ErrorKind::NothingToCommit, "没有已暂存的改动，无法提交。")
    } else {
        (
            ErrorKind::CommandFailed,
            "Git 命令执行失败，详见下方原始输出。",
        )
    };

    let suggestions = match kind {
        ErrorKind::IndexLocked => vec![suggest(
            "确认没有其他 Git 进程占用后重试",
            Risk::Safe,
            "repo.refresh",
        )],
        ErrorKind::CheckoutConflict => vec![suggest("取消切换并先处理改动", Risk::Safe, "manual")],
        ErrorKind::IdentityMissing => vec![suggest(
            "按下面的命令配置 user.name / user.email 后重试",
            Risk::Safe,
            "repo.refresh",
        )],
        ErrorKind::NotARepo => vec![suggest("选择其他目录", Risk::Safe, "repo.pick")],
        _ => vec![suggest("刷新后重试", Risk::Safe, "repo.refresh")],
    };

    let mut error = G4uError::new(kind, human)
        .with_cause(stderr.to_string())
        .with_suggestions(suggestions);
    match &error.kind {
        ErrorKind::IdentityMissing => {
            error = error.with_cli(
                "git config --global user.name \"Your Name\" && git config --global user.email \"you@example.com\"",
            );
        }
        ErrorKind::IndexLocked => error = error.with_cli("rm .git/index.lock"),
        _ => {}
    }
    error
}

// ---------------------------------------------------------------------------
// GitRead
// ---------------------------------------------------------------------------

impl GitRead for SystemGit {
    fn status(&self) -> Result<RepoStatus, G4uError> {
        let head = self.head()?;
        let mid_state = self.mid_state();
        let upstream = head
            .branch
            .as_deref()
            .and_then(|branch| self.upstream(branch));

        // bare 仓库没有工作区：只读引用与 log（04-1 边界）。
        if self.is_bare {
            return Ok(RepoStatus {
                version: 0,
                head,
                upstream,
                entries: Vec::new(),
                conflicts: Vec::new(),
                mid_state,
            });
        }

        let raw = self.run_bytes(&[
            "status",
            "--porcelain=v2",
            "--untracked-files=normal",
            "--ignore-submodules=none",
            "-z",
        ])?;

        let mut entries = Vec::new();
        let mut conflicts = Vec::new();
        let chunks: Vec<&[u8]> = raw.split(|b| *b == 0).collect();
        let mut i = 0;
        while i < chunks.len() {
            let chunk = chunks[i];
            i += 1;
            if chunk.is_empty() {
                continue;
            }
            match chunk[0] {
                b'1' => {
                    let text = String::from_utf8_lossy(chunk);
                    // 1 XY sub mH mI mW hH hI path（splitn 后 path 在第 9 段）
                    let mut parts = text.splitn(9, ' ');
                    let _ = parts.next();
                    let xy = parts.next().unwrap_or("..");
                    for _ in 0..6 {
                        let _ = parts.next();
                    }
                    let path = parts.next().unwrap_or_default().to_string();
                    if path.is_empty() {
                        continue;
                    }
                    let (index_state, worktree_state) = parse_xy(xy);
                    entries.push(StatusEntry {
                        path,
                        orig_path: None,
                        index_state,
                        worktree_state,
                        staged: index_state != ChangeKind::Unmodified,
                        additions: 0,
                        deletions: 0,
                    });
                }
                b'2' => {
                    let text = String::from_utf8_lossy(chunk);
                    // 2 XY sub mH mI mW hH hI Xscore path（path 在第 10 段）
                    let mut parts = text.splitn(10, ' ');
                    let _ = parts.next();
                    let xy = parts.next().unwrap_or("..");
                    for _ in 0..7 {
                        let _ = parts.next();
                    }
                    let path = parts.next().unwrap_or_default().to_string();
                    let orig = chunks
                        .get(i)
                        .map(|s| String::from_utf8_lossy(s).into_owned())
                        .unwrap_or_default();
                    i += 1;
                    let (index_state, worktree_state) = parse_xy(xy);
                    if path.is_empty() {
                        continue;
                    }
                    entries.push(StatusEntry {
                        path,
                        orig_path: if orig.is_empty() { None } else { Some(orig) },
                        index_state,
                        worktree_state,
                        staged: index_state != ChangeKind::Unmodified,
                        additions: 0,
                        deletions: 0,
                    });
                }
                b'u' => {
                    let text = String::from_utf8_lossy(chunk);
                    // u XY sub m1 m2 m3 mW h1 h2 h3 path（path 在第 11 段）
                    let mut parts = text.splitn(11, ' ');
                    let _ = parts.next();
                    let _ = parts.next();
                    for _ in 0..8 {
                        let _ = parts.next();
                    }
                    let path = parts.next().unwrap_or_default().to_string();
                    if path.is_empty() {
                        continue;
                    }
                    conflicts.push(path.clone());
                    entries.push(StatusEntry {
                        path: path.clone(),
                        orig_path: None,
                        index_state: ChangeKind::Unmerged,
                        worktree_state: ChangeKind::Unmerged,
                        staged: false,
                        additions: 0,
                        deletions: 0,
                    });
                }
                b'?' => {
                    let path = String::from_utf8_lossy(&chunk[2..]).into_owned();
                    if path.is_empty() {
                        continue;
                    }
                    entries.push(StatusEntry {
                        path,
                        orig_path: None,
                        index_state: ChangeKind::Unmodified,
                        worktree_state: ChangeKind::Untracked,
                        staged: false,
                        additions: 0,
                        deletions: 0,
                    });
                }
                _ => {}
            }
        }

        // 行级统计：worktree vs index + index vs HEAD，两者相加。
        let mut stats: std::collections::HashMap<String, (u32, u32)> =
            std::collections::HashMap::new();
        for args in [
            vec!["diff", "--numstat", "-z", "--no-color"],
            vec!["diff", "--cached", "--numstat", "-z", "--no-color"],
        ] {
            if let Ok(list) = self.numstat(&args) {
                for (path, add, del) in list {
                    let slot = stats.entry(path).or_insert((0, 0));
                    slot.0 += add;
                    slot.1 += del;
                }
            }
        }
        for entry in &mut entries {
            if let Some((add, del)) = stats.get(&entry.path) {
                entry.additions = *add;
                entry.deletions = *del;
            }
        }

        let upstream = head
            .branch
            .as_deref()
            .and_then(|branch| self.upstream(branch));

        Ok(RepoStatus {
            version: 0,
            head,
            upstream,
            entries,
            conflicts,
            mid_state,
        })
    }

    fn log(&self, limit: usize) -> Result<Vec<CommitSummary>, G4uError> {
        if self.head()?.oid.is_none() {
            return Ok(Vec::new());
        }
        let limit = limit.clamp(1, 5000).to_string();
        let raw = self.run_bytes(&[
            "log",
            "--exclude=refs/git4u/*",
            "--all",
            "--topo-order",
            "--no-color",
            "-n",
            &limit,
            "--format=%H%x00%P%x00%an%x00%ae%x00%ct%x00%s%x00%D%x1e",
        ])?;

        let mut commits = Vec::new();
        for record in raw.split(|b| *b == 0x1e) {
            if record.is_empty() {
                continue;
            }
            let fields: Vec<&[u8]> = record.split(|b| *b == 0).collect();
            let get = |idx: usize| -> String {
                fields
                    .get(idx)
                    .map(|s| String::from_utf8_lossy(s).trim().to_string())
                    .unwrap_or_default()
            };
            let oid = get(0);
            if oid.is_empty() {
                continue;
            }
            let parents = get(1).split_whitespace().map(|s| s.to_string()).collect();
            let refs = get(6)
                .split(", ")
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();
            commits.push(CommitSummary {
                short_oid: oid.chars().take(7).collect(),
                oid,
                parents,
                author_name: get(2),
                author_email: get(3),
                committed_at: get(4).parse().unwrap_or(0),
                summary: get(5),
                refs,
            });
        }
        Ok(commits)
    }

    fn branches(&self) -> Result<BranchListing, G4uError> {
        let raw = self.run_bytes(&[
            "for-each-ref",
            "--format=%(refname)%00%(objectname)%00%(HEAD)%00%(upstream:short)%00%(upstream:track)%00%(committerdate:unix)%1e",
            "refs/heads",
            "refs/remotes",
        ])?;

        let mut branches = Vec::new();
        for record in raw.split(|b| *b == 0x1e) {
            if record.is_empty() {
                continue;
            }
            let fields: Vec<&[u8]> = record.split(|b| *b == 0).collect();
            let get = |idx: usize| -> String {
                fields
                    .get(idx)
                    .map(|s| String::from_utf8_lossy(s).trim().to_string())
                    .unwrap_or_default()
            };
            let full_name = get(0);
            if full_name.is_empty() || full_name.ends_with("/HEAD") {
                continue;
            }
            let is_remote = full_name.starts_with("refs/remotes/");
            let short = if is_remote {
                full_name.trim_start_matches("refs/remotes/").to_string()
            } else {
                full_name.trim_start_matches("refs/heads/").to_string()
            };
            let remote = if is_remote {
                short.split('/').next().map(|s| s.to_string())
            } else {
                None
            };
            let track = get(4);
            let (ahead, behind, gone) = parse_track(&track);
            branches.push(BranchInfo {
                name: short,
                full_name,
                oid: get(1),
                is_current: get(2) == "*",
                is_remote,
                remote,
                upstream: {
                    let up = get(3);
                    if up.is_empty() {
                        None
                    } else {
                        Some(up)
                    }
                },
                track: if track.is_empty() { None } else { Some(track) },
                ahead,
                behind,
                gone,
                updated_at: get(5).parse().unwrap_or(0),
            });
        }

        let tag_raw = self.run_bytes(&[
            "for-each-ref",
            "--format=%(refname:short)%00%(objectname)%00%(subject)%1e",
            "refs/tags",
        ])?;
        let mut tags = Vec::new();
        for record in tag_raw.split(|b| *b == 0x1e) {
            if record.is_empty() {
                continue;
            }
            let fields: Vec<&[u8]> = record.split(|b| *b == 0).collect();
            let get = |idx: usize| -> String {
                fields
                    .get(idx)
                    .map(|s| String::from_utf8_lossy(s).trim().to_string())
                    .unwrap_or_default()
            };
            let name = get(0);
            if name.is_empty() {
                continue;
            }
            tags.push(TagInfo {
                name,
                oid: get(1),
                subject: get(2),
            });
        }

        Ok(BranchListing { branches, tags })
    }

    fn diff(&self, path: &str, staged: bool) -> Result<FileDiff, G4uError> {
        let tracked = self
            .execute(&["ls-files", "--error-unmatch", "--", path], None)
            .map(|o| o.status.success())
            .unwrap_or(false);

        if !tracked && !staged {
            return untracked_diff(&self.workdir, path);
        }

        let mut args = vec!["diff", "--no-color", "--no-ext-diff", "--unified=3"];
        if staged {
            args.push("--cached");
        }
        args.push("--");
        args.push(path);
        let text = self.run(&args)?;
        Ok(parse_unified_diff(path, &text))
    }

    fn commit_detail(&self, oid: &str) -> Result<CommitDetail, G4uError> {
        let meta = self.run(&[
            "show",
            "-s",
            "--no-color",
            "--format=%H%x00%P%x00%an%x00%ae%x00%ct%x00%s%x00%b",
            oid,
        ])?;
        let fields: Vec<&str> = meta.split('\0').collect();
        let get = |idx: usize| {
            fields
                .get(idx)
                .copied()
                .unwrap_or_default()
                .trim()
                .to_string()
        };
        let full_oid = get(0);
        let commit = CommitSummary {
            short_oid: full_oid.chars().take(7).collect(),
            oid: full_oid,
            parents: get(1).split_whitespace().map(|s| s.to_string()).collect(),
            summary: get(5),
            author_name: get(2),
            author_email: get(3),
            committed_at: get(4).parse().unwrap_or(0),
            refs: Vec::new(),
        };

        let name_status = self
            .run(&["show", "--name-status", "--no-color", "--format=", oid])
            .unwrap_or_default();
        let mut kinds: std::collections::HashMap<String, ChangeKind> =
            std::collections::HashMap::new();
        for line in name_status.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let mut parts = line.split('\t');
            let code = parts.next().unwrap_or_default();
            let kind = match code.chars().next() {
                Some('A') => ChangeKind::Added,
                Some('D') => ChangeKind::Deleted,
                Some('R') => ChangeKind::Renamed,
                Some('C') => ChangeKind::Copied,
                _ => ChangeKind::Modified,
            };
            let path = if kind == ChangeKind::Renamed || kind == ChangeKind::Copied {
                parts.nth(1).unwrap_or_default()
            } else {
                parts.next().unwrap_or_default()
            };
            if !path.is_empty() {
                kinds.insert(path.to_string(), kind);
            }
        }

        let mut files = Vec::new();
        if let Ok(nums) = self.numstat(&["show", "--numstat", "-z", "--format=", oid]) {
            for (path, add, del) in nums {
                let change = kinds.remove(&path).unwrap_or(ChangeKind::Modified);
                files.push(FileStat {
                    path,
                    change,
                    additions: add,
                    deletions: del,
                });
            }
        }

        Ok(CommitDetail {
            commit,
            body: get(6),
            files,
        })
    }
}

// ---------------------------------------------------------------------------
// GitWrite
// ---------------------------------------------------------------------------

impl GitWrite for SystemGit {
    fn stage(&self, paths: &[String]) -> Result<(), G4uError> {
        let mut args = vec!["add", "-A", "--"];
        if paths.is_empty() {
            args.push(".");
        } else {
            args.extend(paths.iter().map(|s| s.as_str()));
        }
        self.run(&args)?;
        Ok(())
    }

    fn unstage(&self, paths: &[String]) -> Result<(), G4uError> {
        let unborn = self.head()?.oid.is_none();
        if unborn {
            // 无 HEAD 时 restore --staged 没有基线，改用 rm --cached 保留工作区文件。
            let mut args = vec!["rm", "-r", "--cached", "--quiet", "--"];
            if paths.is_empty() {
                args.push(".");
            } else {
                args.extend(paths.iter().map(|s| s.as_str()));
            }
            let _ = self.run(&args);
        } else {
            let mut args = vec!["restore", "--staged", "--"];
            if paths.is_empty() {
                args.push(".");
            } else {
                args.extend(paths.iter().map(|s| s.as_str()));
            }
            self.run(&args)?;
        }
        Ok(())
    }

    fn commit(&self, message: &str, amend: bool, run_hooks: bool) -> Result<Oid, G4uError> {
        if message.trim().is_empty() {
            return Err(G4uError::new(
                ErrorKind::EmptyMessage,
                "提交信息不能为空。写清楚「做了什么、为什么」。",
            ));
        }
        let mut args = vec!["commit", "-F", "-"];
        if amend {
            args.push("--amend");
        }
        if !run_hooks {
            args.push("--no-verify");
        }
        let out = self.run_with_stdin(&args, message)?;
        if out.contains("nothing to commit") {
            return Err(G4uError::new(
                ErrorKind::NothingToCommit,
                "没有已暂存的改动，无法提交。",
            ));
        }
        self.rev_parse_head()
    }

    fn checkout_branch(&self, name: &str) -> Result<(), G4uError> {
        self.run(&["switch", name])?;
        Ok(())
    }

    fn create_branch(
        &self,
        name: &str,
        start: Option<&str>,
        checkout: bool,
    ) -> Result<(), G4uError> {
        let name = name.trim();
        if self
            .execute(&["check-ref-format", "--branch", name], None)
            .map(|o| !o.status.success())
            .unwrap_or(true)
        {
            return Err(G4uError::new(
                ErrorKind::CommandFailed,
                format!("分支名「{name}」不合法。"),
            ));
        }
        if checkout {
            let mut args = vec!["switch", "-c", name];
            if let Some(start) = start {
                args.push(start);
            }
            self.run(&args)?;
        } else {
            let mut args = vec!["branch", name];
            if let Some(start) = start {
                args.push(start);
            }
            self.run(&args)?;
        }
        Ok(())
    }

    fn fetch(
        &self,
        remote: Option<&str>,
        credential: Option<&AskpassCredential>,
        proxy: Option<&str>,
    ) -> Result<String, G4uError> {
        let mut args = vec!["fetch", "--prune"];
        if let Some(remote) = remote {
            args.push(remote);
        }
        self.run_with_credential(&args, credential, proxy)
    }

    fn pull(
        &self,
        strategy: PullStrategy,
        credential: Option<&AskpassCredential>,
        proxy: Option<&str>,
    ) -> Result<(), G4uError> {
        let mode = match strategy {
            PullStrategy::Merge => "--no-rebase",
            PullStrategy::Rebase => "--rebase",
            PullStrategy::FfOnly => "--ff-only",
        };
        self.run_with_credential(&["pull", mode], credential, proxy)?;
        Ok(())
    }

    fn push(
        &self,
        remote: &str,
        branch: &str,
        set_upstream: bool,
        credential: Option<&AskpassCredential>,
        proxy: Option<&str>,
    ) -> Result<(), G4uError> {
        let mut args = vec!["push"];
        if set_upstream {
            args.push("--set-upstream");
        }
        args.push(remote);
        args.push(branch);
        self.run_with_credential(&args, credential, proxy)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 解析工具
// ---------------------------------------------------------------------------

fn parse_xy(xy: &str) -> (ChangeKind, ChangeKind) {
    let mut chars = xy.chars();
    let x = map_status_char(chars.next().unwrap_or('.'));
    let y = map_status_char(chars.next().unwrap_or('.'));
    (x, y)
}

fn map_status_char(c: char) -> ChangeKind {
    match c {
        'M' => ChangeKind::Modified,
        'A' => ChangeKind::Added,
        'D' => ChangeKind::Deleted,
        'R' => ChangeKind::Renamed,
        'C' => ChangeKind::Copied,
        'U' => ChangeKind::Unmerged,
        '?' => ChangeKind::Untracked,
        _ => ChangeKind::Unmodified,
    }
}

fn untracked_diff(workdir: &Path, path: &str) -> Result<FileDiff, G4uError> {
    let full = workdir.join(path);
    let bytes = match fs::read(&full) {
        Ok(bytes) => bytes,
        Err(_) => return Ok(FileDiff::empty(path)),
    };
    let mut diff = FileDiff::empty(path);
    if bytes.len() > 2 * 1024 * 1024 || bytes.iter().take(8192).any(|b| *b == 0) {
        diff.binary = true;
        return Ok(diff);
    }

    let text = String::from_utf8_lossy(&bytes);
    let mut lines: Vec<&str> = text.split('\n').collect();
    if text.ends_with('\n') {
        lines.pop();
    }
    let count = lines.len() as u32;
    let mut hunk = DiffHunk {
        header: format!("@@ -0,0 +1,{count} @@"),
        old_start: 0,
        old_lines: 0,
        new_start: 1,
        new_lines: count,
        lines: Vec::new(),
    };
    for (idx, line) in lines.iter().enumerate() {
        diff.additions += 1;
        hunk.lines.push(DiffLine {
            kind: DiffLineKind::Add,
            old_lineno: None,
            new_lineno: Some(idx as u32 + 1),
            content: line.strip_suffix('\r').unwrap_or(line).to_string(),
        });
    }
    if count > 0 {
        diff.hunks.push(hunk);
    }
    Ok(diff)
}

fn parse_unified_diff(path: &str, text: &str) -> FileDiff {
    let mut diff = FileDiff::empty(path);
    let mut current: Option<DiffHunk> = None;
    let mut old_lineno = 0u32;
    let mut new_lineno = 0u32;

    for raw in text.lines() {
        if raw.starts_with("@@") {
            if let Some(hunk) = current.take() {
                diff.hunks.push(hunk);
            }
            let (hunk, old, new) = parse_hunk_header(raw).unwrap_or_else(|| {
                (
                    DiffHunk {
                        header: raw.to_string(),
                        old_start: 0,
                        old_lines: 0,
                        new_start: 0,
                        new_lines: 0,
                        lines: Vec::new(),
                    },
                    0,
                    0,
                )
            });
            old_lineno = old;
            new_lineno = new;
            current = Some(hunk);
            continue;
        }

        if raw.starts_with("Binary files") || raw.starts_with("GIT binary patch") {
            diff.binary = true;
            continue;
        }

        let Some(hunk) = current.as_mut() else {
            if let Some(rest) = raw.strip_prefix("--- ") {
                if rest != "/dev/null" {
                    diff.old_path = Some(rest.trim_start_matches("a/").to_string());
                }
            }
            continue;
        };

        if raw.starts_with('\\') {
            continue;
        }
        let (kind, content) = if let Some(rest) = raw.strip_prefix('+') {
            (DiffLineKind::Add, rest)
        } else if let Some(rest) = raw.strip_prefix('-') {
            (DiffLineKind::Del, rest)
        } else if let Some(rest) = raw.strip_prefix(' ') {
            (DiffLineKind::Context, rest)
        } else if raw.is_empty() {
            (DiffLineKind::Context, "")
        } else {
            // 非 hunk 行（文件头 / 新 hunk 之间的元数据）结束当前 hunk。
            diff.hunks.push(current.take().unwrap());
            continue;
        };

        let (old_no, new_no) = match kind {
            DiffLineKind::Add => {
                diff.additions += 1;
                let no = Some(new_lineno);
                new_lineno += 1;
                (None, no)
            }
            DiffLineKind::Del => {
                diff.deletions += 1;
                let no = Some(old_lineno);
                old_lineno += 1;
                (no, None)
            }
            DiffLineKind::Context => {
                let old_no = Some(old_lineno);
                let new_no = Some(new_lineno);
                old_lineno += 1;
                new_lineno += 1;
                (old_no, new_no)
            }
        };
        hunk.lines.push(DiffLine {
            kind,
            old_lineno: old_no,
            new_lineno: new_no,
            content: content.strip_suffix('\r').unwrap_or(content).to_string(),
        });
    }
    if let Some(hunk) = current.take() {
        diff.hunks.push(hunk);
    }
    diff
}

fn parse_hunk_header(line: &str) -> Option<(DiffHunk, u32, u32)> {
    // @@ -12,7 +12,9 @@ context
    let mut parts = line.split_whitespace();
    let _ = parts.next(); // @@
    let old = parts.next()?.trim_start_matches('-');
    let new = parts.next()?.trim_start_matches('+');
    let (old_start, old_lines) = parse_range(old);
    let (new_start, new_lines) = parse_range(new);
    Some((
        DiffHunk {
            header: line.to_string(),
            old_start,
            old_lines,
            new_start,
            new_lines,
            lines: Vec::new(),
        },
        old_start,
        new_start,
    ))
}

fn parse_range(raw: &str) -> (u32, u32) {
    let mut parts = raw.split(',');
    let start = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let lines = parts.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    (start, lines)
}

/// 统计目录占用；只跟随真实文件，跳过符号链接避免环路。
fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| match entry.file_type() {
            Ok(kind) if kind.is_dir() => dir_size(&entry.path()),
            Ok(kind) if kind.is_file() => entry.metadata().map(|m| m.len()).unwrap_or(0),
            _ => 0,
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_backup_timestamp_from_ref_name() {
        let ts = parse_backup_timestamp("refs/git4u/backup/20261108-153012-rebase-7f3a").unwrap();
        assert!(ts > 1_700_000_000);
        assert!(parse_backup_timestamp("refs/git4u/backup/no-timestamp").is_none());
        assert!(parse_backup_timestamp("refs/g4u/backup/20261108-153012-rebase").is_none());
    }

    #[test]
    fn generated_backup_stamp_is_cleanup_parseable() {
        let name = format!("refs/git4u/backup/{}-amend", backup_stamp());
        assert!(parse_backup_timestamp(&name).is_some(), "{name}");
    }

    #[test]
    fn proxy_env_sets_and_clears() {
        let mut cmd = Command::new("git");
        apply_proxy(&mut cmd, Some("socks5://127.0.0.1:1080"));
        let envs: Vec<(String, Option<String>)> = cmd
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().into_owned(),
                    value.map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect();
        assert!(
            envs.iter()
                .any(|(key, value)| key.eq_ignore_ascii_case("HTTPS_PROXY")
                    && value.as_deref() == Some("socks5://127.0.0.1:1080")),
            "envs={envs:?}"
        );

        let mut direct = Command::new("git");
        apply_proxy(&mut direct, None);
        let envs: Vec<(String, Option<String>)> = direct
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().into_owned(),
                    value.map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect();
        assert!(envs.iter().any(|(key, value)| {
            key.eq_ignore_ascii_case("NO_PROXY") && value.as_deref() == Some("*")
        }));
        assert!(!envs
            .iter()
            .any(|(key, value)| { key.eq_ignore_ascii_case("HTTPS_PROXY") && value.is_some() }));
    }

    #[test]
    fn parses_hunk_header() {
        let (hunk, old, new) = parse_hunk_header("@@ -12,7 +12,9 @@ fn login()").unwrap();
        assert_eq!((hunk.old_start, hunk.old_lines), (12, 7));
        assert_eq!((hunk.new_start, hunk.new_lines), (12, 9));
        assert_eq!((old, new), (12, 12));
    }

    #[test]
    fn parses_track_counts() {
        assert_eq!(parse_track("[ahead 2, behind 1]"), (2, 1, false));
        assert_eq!(parse_track("[gone]"), (0, 0, true));
        assert_eq!(parse_track(""), (0, 0, false));
    }

    #[test]
    fn roundtrip_temp_repo() {
        let dir = std::env::temp_dir().join(format!(
            "g4u-core-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str]| {
            let out = Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(args)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git {:?}: {}",
                args,
                String::from_utf8_lossy(&out.stderr)
            );
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "test@example.com"]);
        git(&["config", "user.name", "Test"]);

        let repo = SystemGit::discover(&dir).unwrap();
        assert!(repo.status().unwrap().entries.is_empty());

        fs::write(dir.join("hello.txt"), "one\ntwo\n").unwrap();
        let status = repo.status().unwrap();
        assert_eq!(status.entries.len(), 1);
        assert_eq!(status.entries[0].path, "hello.txt");
        assert_eq!(status.entries[0].worktree_state, ChangeKind::Untracked);

        repo.stage(&[]).unwrap();
        assert_eq!(repo.status().unwrap().staged_count(), 1);

        let oid = repo.commit("init: hello\n", false, true).unwrap();
        assert_eq!(oid.len(), 40);
        assert!(repo.status().unwrap().entries.is_empty());

        fs::write(dir.join("hello.txt"), "one\ntwo\nthree\n").unwrap();
        let modified = repo.status().unwrap();
        assert_eq!(modified.entries[0].path, "hello.txt");
        assert!(modified.entries[0].additions >= 1);
        let diff = repo.diff("hello.txt", false).unwrap();
        assert_eq!(diff.additions, 1);
        assert_eq!(diff.hunks.len(), 1);

        let log = repo.log(10).unwrap();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].oid, oid);

        let stats = repo.stats();
        assert_eq!(stats.commit_count, 1);
        assert_eq!(stats.branch_count, 1);
        assert!(stats.last_commit_at.unwrap_or(0) > 0);
        assert_eq!(stats.last_commit_summary.as_deref(), Some("init: hello"));
        assert!(stats.size_bytes > 0);

        let detail = repo.commit_detail(&oid).unwrap();
        assert_eq!(detail.files.len(), 1);
        assert_eq!(detail.files[0].path, "hello.txt");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn parses_unified_diff_lines() {
        let diff = parse_unified_diff("src/a.rs", "@@ -1,3 +1,4 @@\n a\n-b\n+c\n+d\n e\n");
        assert_eq!(diff.additions, 2);
        assert_eq!(diff.deletions, 1);
        assert_eq!(diff.hunks.len(), 1);
        assert_eq!(diff.hunks[0].lines.len(), 5);
        assert_eq!(diff.hunks[0].lines[2].new_lineno, Some(2));
        assert_eq!(diff.hunks[0].lines[4].new_lineno, Some(4));
    }
}
