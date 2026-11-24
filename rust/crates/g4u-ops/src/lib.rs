//! g4u-ops — 应用服务层：仓库会话池、版本号、写互斥与用例编排。
//!
//! shortcut: 安全层先覆盖 commit/amend（备份引用 + JSONL Journal + soft undo）；
//! reset/删分支/stash/push 的 Precheck 与撤销在后续切片接入。
//! 这里保持 ops 是唯一的写入口，后续安全层只在此层叠加，前端无需改动。

use std::collections::{HashMap, HashSet};
use std::fs;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use g4u_core::{git_version, AskpassCredential, GitRead, GitWrite, SystemGit};
use g4u_cred::CredentialStore;
use g4u_model::{
    suggest, suggest_target, AppInfo, AuditFinding, BackupRef, BranchListing, CommitDetail,
    CommitSummary, CredentialEntry, CredentialInput, CredentialValidation, EnvReport, EolReport,
    ErrorKind, FileDiff, FilePreview, G4uError, GitignoreFile, IgnoreMatch, LargeFile, LockInfo,
    LockReport, ObjectStats, Oid, ProxyEntry, ProxyInput, PushAuditReport, ReflogEntry, RemoteInfo,
    RepoId, RepoSettings, RepoStatus, RepoSummary, Risk, SecretFinding, SettingsScope,
    SettingsView,
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Default, Serialize, Deserialize)]
struct SettingsFile {
    #[serde(default)]
    global: RepoSettings,
    #[serde(default)]
    repos: HashMap<RepoId, RepoSettings>,
    #[serde(default)]
    proxies: Vec<ProxyEntry>,
}

struct Session {
    git: SystemGit,
    version: AtomicU64,
    write_lock: Mutex<()>,
}

impl Session {
    fn new(git: SystemGit) -> Self {
        Self {
            git,
            version: AtomicU64::new(1),
            write_lock: Mutex::new(()),
        }
    }

    fn bump(&self) -> u64 {
        self.version.fetch_add(1, Ordering::SeqCst) + 1
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct JournalEntry {
    repo_id: String,
    at: u64,
    op: String,
    before: Option<String>,
    after: Option<String>,
    backup_ref: Option<String>,
    undoable: bool,
}

pub struct RepoService {
    state_dir: PathBuf,
    sessions: Mutex<HashMap<RepoId, Arc<Session>>>,
    /// 已添加仓库（持久化到 repos.json，按添加顺序）。
    paths: Mutex<Vec<String>>,
    settings: Mutex<SettingsFile>,
    credentials: CredentialStore,
}

impl RepoService {
    pub fn new(state_dir: PathBuf) -> Self {
        let paths = load_paths(&state_dir);
        let settings = load_settings(&state_dir);
        let credentials = CredentialStore::new(state_dir.clone());
        Self {
            state_dir,
            sessions: Mutex::new(HashMap::new()),
            paths: Mutex::new(paths),
            settings: Mutex::new(settings),
            credentials,
        }
    }

    fn journal_path(&self) -> PathBuf {
        self.state_dir.join("journal.jsonl")
    }

    fn append_journal(&self, entry: JournalEntry) -> Result<(), G4uError> {
        let line = serde_json::to_string(&entry)
            .map_err(|err| G4uError::internal(format!("序列化 Journal 失败: {err}")))?;
        fs::create_dir_all(&self.state_dir)
            .map_err(|err| G4uError::internal(format!("创建数据目录失败: {err}")))?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.journal_path())
            .map_err(|err| G4uError::internal(format!("打开 Journal 失败: {err}")))?;
        writeln!(file, "{line}")
            .map_err(|err| G4uError::internal(format!("写入 Journal 失败: {err}")))
    }

    fn journal_entries(&self) -> Vec<JournalEntry> {
        // shortcut: 每次撤销读整个 JSONL；记录量按日增长后再做尾部索引/轮转。
        fs::read_to_string(self.journal_path())
            .map(|text| {
                text.lines()
                    .filter_map(|line| serde_json::from_str(line).ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    // -----------------------------------------------------------------------
    // 仓库注册表
    // -----------------------------------------------------------------------

    pub fn list(&self) -> Vec<RepoSummary> {
        let paths = self.paths.lock().unwrap().clone();
        paths.iter().map(|path| self.summary_of(path)).collect()
    }

    pub fn open(&self, path: &str) -> Result<RepoSummary, G4uError> {
        let git = SystemGit::discover(Path::new(path))?;
        let canonical = canonical(&git.workdir);
        let id = repo_id(&canonical);

        let summary = self.summary_for(&id, git.clone());
        self.remember(&id, &git, canonical);
        Ok(summary)
    }

    pub fn remove(&self, id: &str) -> Result<(), G4uError> {
        {
            let mut sessions = self.sessions.lock().unwrap();
            sessions.remove(id);
        }
        {
            let mut paths = self.paths.lock().unwrap();
            paths.retain(|p| repo_id(&canonical(Path::new(p))) != id);
            self.persist(&paths)?;
        }
        Ok(())
    }

    fn remember(&self, id: &str, git: &SystemGit, canonical_path: String) {
        {
            let mut sessions = self.sessions.lock().unwrap();
            sessions
                .entry(id.to_string())
                .or_insert_with(|| Arc::new(Session::new(git.clone())));
        }
        {
            let mut paths = self.paths.lock().unwrap();
            if !paths
                .iter()
                .any(|p| canonical(Path::new(p)) == canonical_path)
            {
                paths.push(canonical_path);
                let _ = self.persist(&paths);
            }
        }
    }

    fn persist(&self, paths: &[String]) -> Result<(), G4uError> {
        fs::create_dir_all(&self.state_dir)
            .map_err(|e| G4uError::internal(format!("创建数据目录失败: {e}")))?;
        let file = self.state_dir.join("repos.json");
        let json = serde_json::to_string_pretty(paths)
            .map_err(|e| G4uError::internal(format!("序列化仓库列表失败: {e}")))?;
        fs::write(&file, json).map_err(|e| G4uError::internal(format!("写入仓库列表失败: {e}")))
    }

    fn session(&self, id: &str) -> Result<Arc<Session>, G4uError> {
        if let Some(session) = self.sessions.lock().unwrap().get(id) {
            return Ok(session.clone());
        }
        // 会话池可被进程重启清空：按持久化路径重建。
        let paths = self.paths.lock().unwrap().clone();
        for path in paths {
            if repo_id(&canonical(Path::new(&path))) != id {
                continue;
            }
            let git = SystemGit::discover(Path::new(&path))?;
            let mut sessions = self.sessions.lock().unwrap();
            return Ok(sessions
                .entry(id.to_string())
                .or_insert_with(|| Arc::new(Session::new(git)))
                .clone());
        }
        Err(G4uError::new(
            ErrorKind::NoSuchRepo,
            "仓库未打开或已被移除，请从左侧重新打开。",
        ))
    }

    fn summary_of(&self, path: &str) -> RepoSummary {
        let canonical_path = canonical(Path::new(path));
        let id = repo_id(&canonical_path);
        match SystemGit::discover(Path::new(path)) {
            Ok(git) => {
                let mut summary = self.summary_for(&id, git);
                summary.valid = true;
                summary
            }
            Err(err) => RepoSummary {
                id,
                name: Path::new(path)
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.to_string()),
                path: path.to_string(),
                valid: false,
                head: None,
                staged: 0,
                unstaged: 0,
                conflicts: 0,
                ahead: 0,
                behind: 0,
                mid_state: None,
                stats: None,
                error: Some(err.human),
            },
        }
    }

    fn summary_for(&self, id: &str, git: SystemGit) -> RepoSummary {
        let head = git.head().ok();
        let status = git.status().ok();
        let (staged, unstaged, conflicts, mid_state, ahead, behind) = status
            .as_ref()
            .map(|s| {
                let upstream = s.upstream.as_ref();
                (
                    s.staged_count(),
                    s.unstaged_count(),
                    s.conflicts.len(),
                    s.mid_state.clone(),
                    upstream.map(|u| u.ahead).unwrap_or(0),
                    upstream.map(|u| u.behind).unwrap_or(0),
                )
            })
            .unwrap_or((0, 0, 0, None, 0, 0));
        let stats = git.stats();

        RepoSummary {
            id: id.to_string(),
            name: git.name(),
            path: git.workdir.to_string_lossy().into_owned(),
            valid: true,
            head,
            staged,
            unstaged,
            conflicts,
            ahead,
            behind,
            mid_state,
            stats: Some(stats),
            error: None,
        }
    }

    // -----------------------------------------------------------------------
    // 读用例
    // -----------------------------------------------------------------------

    pub fn status(&self, id: &str) -> Result<RepoStatus, G4uError> {
        let session = self.session(id)?;
        let mut status = session.git.status()?;
        status.version = session.bump();
        Ok(status)
    }

    pub fn log(&self, id: &str, limit: usize) -> Result<Vec<CommitSummary>, G4uError> {
        self.session(id)?.git.log(limit)
    }

    pub fn branches(&self, id: &str) -> Result<BranchListing, G4uError> {
        self.session(id)?.git.branches()
    }

    pub fn diff(&self, id: &str, path: &str, staged: bool) -> Result<FileDiff, G4uError> {
        self.session(id)?.git.diff(path, staged)
    }

    pub fn commit_detail(&self, id: &str, oid: &str) -> Result<CommitDetail, G4uError> {
        self.session(id)?.git.commit_detail(oid)
    }

    /// 读取仓库根目录的 `.gitignore`；不存在时返回空内容，供设置模板时追加。
    pub fn gitignore(&self, id: &str) -> Result<GitignoreFile, G4uError> {
        let session = self.session(id)?;
        let path = gitignore_path(&session)?;
        match fs::read_to_string(&path) {
            Ok(content) => Ok(GitignoreFile {
                exists: true,
                content,
            }),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(GitignoreFile {
                exists: false,
                content: String::new(),
            }),
            Err(err) => Err(G4uError::internal(format!("读取 .gitignore 失败: {err}"))),
        }
    }

    /// 设置页「关于」：应用版本由 Tauri 层传入，其余来自本机环境。
    pub fn app_info(&self, app_version: impl Into<String>) -> AppInfo {
        AppInfo {
            app_version: app_version.into(),
            git_version: git_version(),
            engine: "系统 git CLI".into(),
            app_data_dir: self.state_dir.to_string_lossy().into_owned(),
        }
    }

    pub fn settings(&self, id: Option<&str>) -> SettingsView {
        let file = self.settings.lock().unwrap();
        let repo = id.and_then(|id| file.repos.get(id).cloned());
        let effective = repo.clone().unwrap_or_else(|| file.global.clone());
        SettingsView {
            global: file.global.clone(),
            repo,
            effective,
            proxies: file.proxies.clone(),
        }
    }

    pub fn save_settings(
        &self,
        scope: SettingsScope,
        id: Option<&str>,
        next: RepoSettings,
    ) -> Result<SettingsView, G4uError> {
        validate_protected_patterns(&next.protected_branches)?;
        let mut file = self.settings.lock().unwrap();
        if let Some(proxy_id) = &next.proxy_id {
            if !file.proxies.iter().any(|proxy| &proxy.id == proxy_id) {
                return Err(G4uError::new(
                    ErrorKind::CommandFailed,
                    "选择的代理不存在，请刷新设置后重试。",
                ));
            }
        }
        match scope {
            SettingsScope::Global => file.global = next,
            SettingsScope::Repo => {
                let id = id.ok_or_else(|| {
                    G4uError::new(
                        ErrorKind::NoSuchRepo,
                        "请先打开一个仓库，再保存仓库级设置。",
                    )
                })?;
                if !self.has_repo(id) {
                    return Err(G4uError::new(
                        ErrorKind::NoSuchRepo,
                        "仓库未打开或已被移除，无法保存仓库级设置。",
                    ));
                }
                if next == file.global {
                    file.repos.remove(id);
                } else {
                    file.repos.insert(id.to_string(), next);
                }
            }
        }
        self.persist_settings(&file)?;
        let view = {
            let repo = id.and_then(|id| file.repos.get(id).cloned());
            let effective = repo.clone().unwrap_or_else(|| file.global.clone());
            SettingsView {
                global: file.global.clone(),
                repo,
                effective,
                proxies: file.proxies.clone(),
            }
        };
        Ok(view)
    }

    // -----------------------------------------------------------------------
    // 网络代理
    // -----------------------------------------------------------------------

    pub fn reset_settings(&self) -> Result<SettingsView, G4uError> {
        let mut file = self.settings.lock().unwrap();
        file.global = RepoSettings::default();
        file.repos.clear();
        self.persist_settings(&file)?;
        Ok(SettingsView {
            global: file.global.clone(),
            repo: None,
            effective: file.global.clone(),
            proxies: file.proxies.clone(),
        })
    }

    pub fn save_proxy(&self, input: ProxyInput) -> Result<Vec<ProxyEntry>, G4uError> {
        let name = input.name.trim();
        if name.is_empty() {
            return Err(G4uError::new(
                ErrorKind::CommandFailed,
                "代理名称不能为空。",
            ));
        }
        let url = validate_proxy_url(&input.url)?;
        let id = input
            .id
            .filter(|id| !id.trim().is_empty())
            .unwrap_or_else(|| proxy_id(name, &url));
        let entry = ProxyEntry {
            id: id.clone(),
            name: name.to_string(),
            url,
        };
        let mut file = self.settings.lock().unwrap();
        if let Some(slot) = file.proxies.iter_mut().find(|proxy| proxy.id == id) {
            *slot = entry;
        } else {
            file.proxies.push(entry);
        }
        file.proxies.sort_by_key(|proxy| proxy.name.to_lowercase());
        self.persist_settings(&file)?;
        Ok(file.proxies.clone())
    }

    pub fn delete_proxy(&self, id: &str) -> Result<Vec<ProxyEntry>, G4uError> {
        let mut file = self.settings.lock().unwrap();
        file.proxies.retain(|proxy| proxy.id != id);
        if file.global.proxy_id.as_deref() == Some(id) {
            file.global.proxy_id = None;
        }
        let global = file.global.clone();
        for repo in file.repos.values_mut() {
            if repo.proxy_id.as_deref() == Some(id) {
                repo.proxy_id = None;
            }
        }
        file.repos.retain(|_, repo| *repo != global);
        self.persist_settings(&file)?;
        Ok(file.proxies.clone())
    }

    fn proxy_url(&self, id: Option<&str>) -> Option<String> {
        let view = self.settings(id);
        let proxy_id = view.effective.proxy_id?;
        view.proxies
            .into_iter()
            .find(|proxy| proxy.id == proxy_id)
            .map(|proxy| proxy.url)
    }

    fn has_repo(&self, id: &str) -> bool {
        self.paths
            .lock()
            .unwrap()
            .iter()
            .any(|path| repo_id(&canonical(Path::new(path))) == id)
    }

    fn persist_settings(&self, settings: &SettingsFile) -> Result<(), G4uError> {
        fs::create_dir_all(&self.state_dir)
            .map_err(|err| G4uError::internal(format!("创建数据目录失败: {err}")))?;
        let json = serde_json::to_string_pretty(settings)
            .map_err(|err| G4uError::internal(format!("序列化设置失败: {err}")))?;
        let file = self.state_dir.join("settings.json");
        let tmp = self.state_dir.join("settings.json.tmp");
        fs::write(&tmp, json).map_err(|err| G4uError::internal(format!("写入设置失败: {err}")))?;
        fs::rename(&tmp, &file)
            .map_err(|err| G4uError::internal(format!("替换设置文件失败: {err}")))
    }

    // -----------------------------------------------------------------------
    // 账户与凭证
    // -----------------------------------------------------------------------

    pub fn credentials(&self) -> Vec<CredentialEntry> {
        self.credentials.list()
    }

    pub fn save_credential(&self, input: CredentialInput) -> Result<CredentialEntry, G4uError> {
        self.credentials.save(input)
    }

    pub fn delete_credential(&self, id: &str) -> Result<(), G4uError> {
        self.credentials.delete(id)
    }

    pub fn validate_credential(
        &self,
        repo_id: &str,
        id: &str,
    ) -> Result<CredentialValidation, G4uError> {
        let entry = self
            .credentials
            .get(id)
            .ok_or_else(|| G4uError::new(ErrorKind::CommandFailed, "凭证不存在或已被删除。"))?;
        let session = self.session(repo_id)?;
        let matched = self.credentials.secret_for(id)?;
        let credential = AskpassCredential {
            username: matched.username,
            password: matched.password,
        };
        match session.git.ls_remote(
            &entry.url,
            Some(&credential),
            self.proxy_url(Some(repo_id)).as_deref(),
        ) {
            Ok(_) => {
                self.credentials.mark_used(id);
                Ok(CredentialValidation {
                    ok: true,
                    message: "验证通过（只读 ls-remote 探测）。".into(),
                    status: "ok".into(),
                })
            }
            Err(error) => {
                self.credentials.mark_failed(id, "expired");
                Ok(CredentialValidation {
                    ok: false,
                    message: error.human,
                    status: "expired".into(),
                })
            }
        }
    }

    // -----------------------------------------------------------------------
    // 写用例（每仓库写互斥；CAS 由 git 自身对 index/ref 保证）
    // -----------------------------------------------------------------------

    pub fn stage(&self, id: &str, paths: &[String]) -> Result<(), G4uError> {
        let session = self.session(id)?;
        let _guard = session.write_lock.lock().unwrap();
        session.git.stage(paths)?;
        session.bump();
        Ok(())
    }

    /// 写入模板生成的忽略规则；追加时保留现有内容，覆盖时先写临时文件再原子替换。
    pub fn write_gitignore(&self, id: &str, content: &str, append: bool) -> Result<(), G4uError> {
        const MAX_BYTES: usize = 1024 * 1024;
        if content.trim().is_empty() {
            return Err(G4uError::new(
                ErrorKind::CommandFailed,
                "忽略规则不能为空，请至少选择一个模板。",
            ));
        }
        if content.len() > MAX_BYTES {
            return Err(G4uError::new(
                ErrorKind::CommandFailed,
                "忽略规则超过 1 MiB，请精简后再写入。",
            ));
        }

        let session = self.session(id)?;
        let path = gitignore_path(&session)?;
        let _guard = session.write_lock.lock().unwrap();

        let mut next = String::new();
        if append {
            match fs::read_to_string(&path) {
                Ok(existing) => {
                    next.push_str(&existing);
                    if !next.is_empty() && !next.ends_with('\n') {
                        next.push('\n');
                    }
                    if !next.is_empty() {
                        next.push('\n');
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => {
                    return Err(G4uError::internal(format!(
                        "读取现有 .gitignore 失败，未写入: {err}"
                    )))
                }
            }
        }
        next.push_str(content);
        if !next.ends_with('\n') {
            next.push('\n');
        }

        let tmp = path.with_file_name(format!(".gitignore.g4u-{}.tmp", std::process::id()));
        fs::write(&tmp, next)
            .map_err(|err| G4uError::internal(format!("写入临时文件失败: {err}")))?;
        if let Err(err) = fs::rename(&tmp, &path) {
            let _ = fs::remove_file(&tmp);
            return Err(G4uError::internal(format!("替换 .gitignore 失败: {err}")));
        }
        session.bump();
        Ok(())
    }

    pub fn unstage(&self, id: &str, paths: &[String]) -> Result<(), G4uError> {
        let session = self.session(id)?;
        let _guard = session.write_lock.lock().unwrap();
        session.git.unstage(paths)?;
        session.bump();
        Ok(())
    }

    pub fn commit(
        &self,
        id: &str,
        message: &str,
        amend: bool,
        run_hooks: bool,
    ) -> Result<Oid, G4uError> {
        let session = self.session(id)?;
        let _guard = session.write_lock.lock().unwrap();
        let before = session.git.rev_parse_head().ok();
        let backup_ref = if amend {
            match before.as_deref() {
                Some(oid) => Some(session.git.create_backup_ref("amend", oid)?),
                None => None,
            }
        } else {
            None
        };
        let oid = session.git.commit(message, amend, run_hooks)?;
        // Journal 是尽力而为：写失败不推翻已完成的提交，备份引用仍可手工恢复。
        let undoable = before.is_some();
        let _ = self.append_journal(JournalEntry {
            repo_id: id.to_string(),
            at: now_ms(),
            op: if amend {
                "commit.amend"
            } else {
                "commit.create"
            }
            .into(),
            before,
            after: Some(oid.clone()),
            backup_ref,
            undoable,
        });
        session.bump();
        Ok(oid)
    }

    /// 撤销最近一次可撤销的提交/Amend；只做 soft reset，暂存区与工作区保持不变。
    pub fn undo_last(&self, id: &str, expected_after: &str) -> Result<Option<Oid>, G4uError> {
        let Some(last) = self
            .journal_entries()
            .into_iter()
            .rev()
            .find(|entry| entry.repo_id == id)
        else {
            return Ok(None);
        };
        if !last.undoable || last.op == "undo" {
            return Ok(None);
        }
        let (Some(before), Some(after)) = (last.before.clone(), last.after.clone()) else {
            return Ok(None);
        };
        if after != expected_after {
            return Err(G4uError::new(
                ErrorKind::CommandFailed,
                "已有更新的提交或操作，不能撤销这条记录。",
            )
            .with_suggestions(vec![suggest("刷新后重试", Risk::Safe, "repo.refresh")]));
        }

        let session = self.session(id)?;
        let _guard = session.write_lock.lock().unwrap();
        let current = session.git.rev_parse_head()?;
        if current != after {
            return Err(
                G4uError::new(ErrorKind::CommandFailed, "HEAD 已变化，不能撤销这条记录。")
                    .with_suggestions(vec![suggest("刷新后重试", Risk::Safe, "repo.refresh")]),
            );
        }
        let backup_ref = session.git.create_backup_ref("undo", &current)?;
        session.git.reset_soft(&before)?;
        let _ = self.append_journal(JournalEntry {
            repo_id: id.to_string(),
            at: now_ms(),
            op: "undo".into(),
            before: Some(current),
            after: Some(before.clone()),
            backup_ref: Some(backup_ref),
            undoable: false,
        });
        session.bump();
        Ok(Some(before))
    }

    pub fn checkout_branch(&self, id: &str, name: &str) -> Result<(), G4uError> {
        let session = self.session(id)?;
        let _guard = session.write_lock.lock().unwrap();
        session.git.checkout_branch(name)?;
        session.bump();
        Ok(())
    }

    pub fn create_branch(
        &self,
        id: &str,
        name: &str,
        start: Option<&str>,
        checkout: bool,
    ) -> Result<(), G4uError> {
        let session = self.session(id)?;
        let _guard = session.write_lock.lock().unwrap();
        session.git.create_branch(name, start, checkout)?;
        session.bump();
        Ok(())
    }

    // -----------------------------------------------------------------------
    // 同步：Fetch / Pull / Push（写互斥）
    // -----------------------------------------------------------------------

    pub fn remotes(&self, id: &str) -> Result<Vec<RemoteInfo>, G4uError> {
        self.session(id)?.git.remotes()
    }

    pub fn fetch(&self, id: &str, remote: Option<&str>) -> Result<(), G4uError> {
        let session = self.session(id)?;
        let remote = resolve_remote(&session, remote)?;
        let credential = self.credential_for(&session, &remote)?;
        let proxy = self.proxy_url(Some(id));
        let result = {
            let _guard = session.write_lock.lock().unwrap();
            session.git.fetch(
                Some(&remote),
                credential.as_ref().map(|(_, cred)| cred),
                proxy.as_deref(),
            )
        };
        let outcome = finish_sync(&session, &self.credentials, credential, result)?;
        let _ = outcome;
        let _ = self.cleanup_backups(id);
        Ok(())
    }

    pub fn pull(&self, id: &str) -> Result<(), G4uError> {
        let strategy = self.settings(Some(id)).effective.pull_strategy;
        let session = self.session(id)?;
        let branch = current_branch(&session)?;
        let remote = session
            .git
            .upstream(&branch)
            .map(|upstream| upstream.name)
            .and_then(|name| name.split('/').next().map(str::to_string))
            .unwrap_or_else(|| default_remote(&session).unwrap_or_else(|_| "origin".into()));
        let credential = self.credential_for(&session, &remote)?;
        let proxy = self.proxy_url(Some(id));
        let result = {
            let _guard = session.write_lock.lock().unwrap();
            session.git.pull(
                strategy,
                credential.as_ref().map(|(_, cred)| cred),
                proxy.as_deref(),
            )
        };
        finish_sync(&session, &self.credentials, credential, result)?;
        let _ = self.cleanup_backups(id);
        Ok(())
    }

    /// 推送前审计：保护分支、落后远端、疑似密钥、大文件。
    pub fn push_audit(&self, id: &str) -> Result<PushAuditReport, G4uError> {
        let session = self.session(id)?;
        let branch = current_branch(&session)?;
        let settings = self.settings(Some(id)).effective;
        let upstream = session.git.upstream(&branch).filter(|info| !info.gone);

        let (ahead, behind) = upstream
            .as_ref()
            .map(|info| (info.ahead, info.behind))
            .unwrap_or((0, 0));
        let mut blockers = Vec::new();
        let mut warnings = Vec::new();

        if behind > 0 {
            blockers.push(AuditFinding {
                kind: "behind".into(),
                path: None,
                detail: format!("本地落后远端 {behind} 个提交，请先 Pull。"),
            });
        }

        if settings.push_audit && branch_matches(&settings.protected_branches, &branch) {
            blockers.push(AuditFinding {
                kind: "protectedBranch".into(),
                path: None,
                detail: format!("{branch} 命中保护分支规则，推送需要二次确认。"),
            });
        }

        if settings.secret_scan {
            let diff = match &upstream {
                Some(info) => session.git.range_diff(&format!("{}..HEAD", info.name))?,
                None => session.git.head_diff().unwrap_or_default(),
            };
            blockers.extend(scan_secrets(&diff));
        }

        if settings.push_audit {
            if let Some(info) = &upstream {
                for path in session.git.range_files(&format!("{}..HEAD", info.name))? {
                    let Ok(meta) = fs::metadata(session.git.workdir.join(&path)) else {
                        continue;
                    };
                    if meta.is_file() && meta.len() > 5 * 1024 * 1024 {
                        warnings.push(AuditFinding {
                            kind: "largeFile".into(),
                            path: Some(path.clone()),
                            detail: format!("{:.1} MiB", meta.len() as f64 / 1024.0 / 1024.0),
                        });
                    }
                }
            }
        }

        Ok(PushAuditReport {
            branch,
            upstream: upstream.map(|info| info.name),
            ahead,
            behind,
            blockers,
            warnings,
        })
    }

    pub fn push(
        &self,
        id: &str,
        remote: Option<&str>,
        set_upstream: bool,
        allow_risky: bool,
    ) -> Result<(), G4uError> {
        let report = self.push_audit(id)?;
        if !report.can_push() && !allow_risky {
            return Err(G4uError::new(
                ErrorKind::CommandFailed,
                format!(
                    "推送审计未通过（{} 项），请先查看审计详情并确认。",
                    report.blockers.len()
                ),
            )
            .with_suggestions(vec![suggest(
                "查看推送审计详情",
                Risk::Caution,
                "push.audit",
            )]));
        }

        let session = self.session(id)?;
        let remote = match remote {
            Some(remote) if !remote.is_empty() => remote.to_string(),
            _ => default_remote(&session)?,
        };
        let branch = current_branch(&session)?;
        let credential = self.credential_for(&session, &remote)?;
        let proxy = self.proxy_url(Some(id));
        let result = {
            let _guard = session.write_lock.lock().unwrap();
            session.git.push(
                &remote,
                &branch,
                set_upstream,
                credential.as_ref().map(|(_, cred)| cred),
                proxy.as_deref(),
            )
        };
        finish_sync(&session, &self.credentials, credential, result)?;
        let _ = self.cleanup_backups(id);
        Ok(())
    }

    /// 工具页：工作区文件列表（已跟踪 + 未跟踪，排除忽略）。
    pub fn files(&self, id: &str) -> Result<Vec<String>, G4uError> {
        self.session(id)?.git.list_files()
    }

    /// 工具页：只读文件预览；限制大小并拒绝越界路径。
    pub fn file_preview(&self, id: &str, path: &str) -> Result<FilePreview, G4uError> {
        let session = self.session(id)?;
        let resolved = resolve_worktree_file(&session.git.workdir, path)?;
        let size = fs::metadata(&resolved)
            .map_err(|err| {
                G4uError::new(ErrorKind::CommandFailed, format!("无法读取文件信息: {err}"))
            })?
            .len();
        const LIMIT: u64 = 512 * 1024;
        let truncated = size > LIMIT;
        let bytes = if truncated {
            let mut buffer = Vec::with_capacity(LIMIT as usize);
            let file = fs::File::open(&resolved).map_err(|err| {
                G4uError::new(ErrorKind::CommandFailed, format!("无法读取文件: {err}"))
            })?;
            file.take(LIMIT).read_to_end(&mut buffer).map_err(|err| {
                G4uError::new(ErrorKind::CommandFailed, format!("无法读取文件: {err}"))
            })?;
            buffer
        } else {
            fs::read(&resolved).map_err(|err| {
                G4uError::new(ErrorKind::CommandFailed, format!("无法读取文件: {err}"))
            })?
        };
        let binary = bytes.iter().take(8192).any(|byte| *byte == 0);
        let content = if binary {
            String::new()
        } else {
            String::from_utf8_lossy(&bytes).into_owned()
        };
        Ok(FilePreview {
            path: path.to_string(),
            size,
            binary,
            truncated,
            content,
        })
    }

    /// 工具页：按大小降序返回工作区文件，最多 `limit` 条。
    pub fn large_files(&self, id: &str, limit: usize) -> Result<Vec<LargeFile>, G4uError> {
        let session = self.session(id)?;
        let mut files: Vec<LargeFile> = session
            .git
            .list_files()?
            .into_iter()
            .filter_map(|path| {
                let resolved = resolve_worktree_file(&session.git.workdir, &path).ok()?;
                let size = fs::metadata(resolved).ok()?.len();
                (size > 0).then_some(LargeFile { path, size })
            })
            .collect();
        files.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.path.cmp(&b.path)));
        files.truncate(limit.clamp(1, 500));
        Ok(files)
    }

    pub fn backup_refs(&self, id: &str) -> Result<Vec<BackupRef>, G4uError> {
        self.session(id)?.git.backup_refs()
    }

    pub fn ignore_check(&self, id: &str, path: &str) -> Result<Option<IgnoreMatch>, G4uError> {
        let relative = safe_relative_path(path)?;
        self.session(id)?.git.check_ignore(relative)
    }

    pub fn eol_report(&self, id: &str) -> Result<EolReport, G4uError> {
        self.session(id)?.git.eol_report()
    }

    pub fn object_stats(&self, id: &str) -> Result<ObjectStats, G4uError> {
        self.session(id)?.git.object_stats()
    }

    pub fn fsck(&self, id: &str) -> Result<String, G4uError> {
        self.session(id)?.git.fsck()
    }

    pub fn reflog(&self, id: &str, limit: usize) -> Result<Vec<ReflogEntry>, G4uError> {
        self.session(id)?.git.reflog(limit)
    }

    pub fn lock_report(&self, id: &str) -> Result<LockReport, G4uError> {
        let session = self.session(id)?;
        let git_dir = &session.git.git_dir;
        let index_lock = fs::metadata(git_dir.join("index.lock")).ok().map(|meta| {
            let modified_at = meta
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs())
                .unwrap_or(0);
            LockInfo {
                modified_at,
                age_seconds: (now_ms() / 1000).saturating_sub(modified_at),
            }
        });
        let mut state_files = Vec::new();
        for name in [
            "MERGE_HEAD",
            "CHERRY_PICK_HEAD",
            "REVERT_HEAD",
            "BISECT_LOG",
            "rebase-merge",
            "rebase-apply",
        ] {
            if git_dir.join(name).exists() {
                state_files.push(name.to_string());
            }
        }
        Ok(LockReport {
            index_lock,
            mid_state: session.git.mid_state(),
            state_files,
        })
    }

    pub fn env_report(&self, id: &str) -> Result<EnvReport, G4uError> {
        let session = self.session(id)?;
        Ok(EnvReport {
            git_version: git_version(),
            workdir: session.git.workdir.to_string_lossy().into_owned(),
            git_dir: session.git.git_dir.to_string_lossy().into_owned(),
            hooks_path: session.git.config_get("core.hooksPath")?,
            credential_helper: session.git.config_get("credential.helper")?,
            http_proxy: env_value(&["HTTP_PROXY", "http_proxy"]),
            https_proxy: env_value(&["HTTPS_PROXY", "https_proxy"]),
            no_proxy: env_value(&["NO_PROXY", "no_proxy"]),
        })
    }

    pub fn secret_scan(&self, id: &str, limit: usize) -> Result<Vec<SecretFinding>, G4uError> {
        let session = self.session(id)?;
        let patterns = secret_patterns();
        let limit = limit.clamp(1, 500);
        let mut findings = Vec::new();
        for path in session.git.list_files()? {
            if findings.len() >= limit {
                break;
            }
            let Ok(resolved) = resolve_worktree_file(&session.git.workdir, &path) else {
                continue;
            };
            let Ok(meta) = fs::metadata(&resolved) else {
                continue;
            };
            if !meta.is_file() || meta.len() > 256 * 1024 {
                continue;
            }
            let Ok(bytes) = fs::read(&resolved) else {
                continue;
            };
            if bytes.iter().take(8192).any(|byte| *byte == 0) {
                continue;
            }
            let text = String::from_utf8_lossy(&bytes);
            for (line_no, line) in text.lines().enumerate() {
                for (kind, regex) in &patterns {
                    if regex.is_match(line) {
                        findings.push(SecretFinding {
                            path: path.clone(),
                            line: line_no + 1,
                            pattern: (*kind).to_string(),
                        });
                        break;
                    }
                }
                if findings.len() >= limit {
                    break;
                }
            }
        }
        Ok(findings)
    }

    pub fn cleanup_backups(&self, id: &str) -> Result<usize, G4uError> {
        let days = self.settings(Some(id)).effective.backup_retention_days;
        let session = self.session(id)?;
        let _guard = session.write_lock.lock().unwrap();
        let count = session.git.cleanup_backup_refs(days)?;
        if count > 0 {
            session.bump();
        }
        Ok(count)
    }

    fn credential_for(
        &self,
        session: &Session,
        remote: &str,
    ) -> Result<Option<(String, AskpassCredential)>, G4uError> {
        let Some(url) = remote_url(session, remote) else {
            return Ok(None);
        };
        let Some(matched) = self.credentials.matched(&url)? else {
            return Ok(None);
        };
        Ok(Some((
            matched.id,
            AskpassCredential {
                username: matched.username,
                password: matched.password,
            },
        )))
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn load_paths(state_dir: &Path) -> Vec<String> {
    let file = state_dir.join("repos.json");
    let Ok(text) = fs::read_to_string(file) else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<String>>(&text).unwrap_or_default()
}

fn load_settings(state_dir: &Path) -> SettingsFile {
    let file = state_dir.join("settings.json");
    let Ok(text) = fs::read_to_string(file) else {
        return SettingsFile::default();
    };
    serde_json::from_str::<SettingsFile>(&text).unwrap_or_default()
}

fn current_branch(session: &Session) -> Result<String, G4uError> {
    session.git.head()?.branch.ok_or_else(|| {
        G4uError::new(
            ErrorKind::CommandFailed,
            "当前是 detached HEAD，请先切换到一个分支再同步。",
        )
    })
}

fn default_remote(session: &Session) -> Result<String, G4uError> {
    let remotes = session.git.remotes()?;
    if remotes.iter().any(|remote| remote.name == "origin") {
        return Ok("origin".into());
    }
    remotes
        .first()
        .map(|remote| remote.name.clone())
        .ok_or_else(|| {
            G4uError::new(
                ErrorKind::CommandFailed,
                "仓库还没有配置远端，请先添加 remote。",
            )
        })
}

fn resolve_remote(session: &Session, remote: Option<&str>) -> Result<String, G4uError> {
    match remote {
        Some(remote) if !remote.is_empty() => Ok(remote.to_string()),
        _ => default_remote(session),
    }
}

fn remote_url(session: &Session, remote: &str) -> Option<String> {
    session
        .git
        .remotes()
        .ok()?
        .into_iter()
        .find(|item| item.name == remote)
        .map(|item| item.url)
}

fn finish_sync<T>(
    session: &Session,
    store: &CredentialStore,
    credential: Option<(String, AskpassCredential)>,
    result: Result<T, G4uError>,
) -> Result<T, G4uError> {
    match result {
        Ok(value) => {
            if let Some((id, _)) = credential {
                store.mark_used(&id);
            }
            session.bump();
            Ok(value)
        }
        Err(mut error) => {
            if error.kind == ErrorKind::AuthFailed {
                error = match credential {
                    Some((id, _)) => {
                        store.mark_failed(&id, "expired");
                        error.with_suggestions(vec![
                            suggest_target(
                                "更新此地址的凭证",
                                Risk::Safe,
                                "credentials.update",
                                id,
                            ),
                            suggest("刷新后重试", Risk::Safe, "repo.refresh"),
                            suggest(
                                "若使用 SSH，请检查 ~/.ssh/config 与密钥",
                                Risk::Safe,
                                "manual",
                            ),
                        ])
                    }
                    None => error.with_suggestions(vec![
                        suggest("添加或更新此地址的凭证", Risk::Safe, "credentials.update"),
                        suggest("刷新后重试", Risk::Safe, "repo.refresh"),
                        suggest(
                            "若使用 SSH，请检查 ~/.ssh/config 与密钥",
                            Risk::Safe,
                            "manual",
                        ),
                    ]),
                };
            }
            Err(error)
        }
    }
}

fn branch_matches(pattern: &str, branch: &str) -> bool {
    if pattern.trim().is_empty() {
        return false;
    }
    Regex::new(pattern)
        .map(|regex| regex.is_match(branch))
        .unwrap_or(false)
}

fn validate_protected_patterns(pattern: &str) -> Result<(), G4uError> {
    if pattern.trim().is_empty() {
        return Ok(());
    }
    Regex::new(pattern.trim())
        .map(|_| ())
        .map_err(|err| G4uError::new(ErrorKind::CommandFailed, format!("保护分支正则无效：{err}")))
}

fn validate_proxy_url(raw: &str) -> Result<String, G4uError> {
    let url = Url::parse(raw.trim())
        .map_err(|_| G4uError::new(ErrorKind::CommandFailed, "代理地址格式不正确。"))?;
    if !matches!(url.scheme(), "http" | "https" | "socks5" | "socks5h") {
        return Err(G4uError::new(
            ErrorKind::CommandFailed,
            "代理只支持 http、https、socks5 或 socks5h。",
        ));
    }
    if url.host_str().is_none() {
        return Err(G4uError::new(
            ErrorKind::CommandFailed,
            "代理地址缺少主机名。",
        ));
    }
    Ok(url.to_string())
}

fn proxy_id(name: &str, url: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in format!("{name}\n{url}").as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("p{hash:016x}")
}

/// shortcut: 首次 push 无 upstream 时只扫描最新提交，不做全历史扫描。
fn secret_patterns() -> Vec<(&'static str, Regex)> {
    [
        ("privateKey", r"-----BEGIN [A-Z ]*PRIVATE KEY-----"),
        ("awsKey", r"AKIA[0-9A-Z]{16}"),
        ("githubToken", r"gh[pousr]_[A-Za-z0-9]{20,}"),
        ("slackToken", r"xox[baprs]-[A-Za-z0-9-]{10,}"),
        (
            "secretAssignment",
            r#"(?i)(api[_-]?key|secret|token|password)\s*[:=]\s*['"][^'"]{8,}['"]"#,
        ),
    ]
    .iter()
    .filter_map(|(kind, pattern)| Regex::new(pattern).ok().map(|regex| (*kind, regex)))
    .collect()
}

/// shortcut: 首次 push 无 upstream 时只扫描最新提交，不做全历史扫描。
fn scan_secrets(diff: &str) -> Vec<AuditFinding> {
    let compiled = secret_patterns();

    let mut findings = Vec::new();
    let mut seen = HashSet::new();
    let mut path: Option<String> = None;
    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("+++ b/") {
            path = Some(rest.to_string());
            continue;
        }
        if line.starts_with("@@") || !line.starts_with('+') || line.starts_with("+++") {
            continue;
        }
        let content = &line[1..];
        for (kind, regex) in &compiled {
            if !regex.is_match(content) {
                continue;
            }
            let key = (path.clone().unwrap_or_default(), (*kind).to_string());
            if !seen.insert(key) {
                break;
            }
            findings.push(AuditFinding {
                kind: (*kind).into(),
                path: path.clone(),
                detail: "检测到疑似密钥，具体值已隐藏。".into(),
            });
            if findings.len() >= 20 {
                return findings;
            }
            break;
        }
    }
    findings
}

fn canonical(path: &Path) -> String {
    let resolved = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let text = resolved.to_string_lossy().into_owned();
    if cfg!(windows) {
        text.to_lowercase()
    } else {
        text
    }
}

fn repo_id(canonical_path: &str) -> RepoId {
    // FNV-1a 64：稳定、无外部依赖；仓库 id 只需在同一台机器上稳定。
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in canonical_path.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("r{hash:016x}")
}

fn safe_relative_path(relative: &str) -> Result<&str, G4uError> {
    let invalid = relative.is_empty()
        || relative.starts_with('/')
        || relative.contains('\\')
        || relative.contains(':')
        || relative
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == ".." || part == ".git");
    if invalid {
        return Err(G4uError::new(
            ErrorKind::CommandFailed,
            "文件路径不合法或超出仓库范围。",
        ));
    }
    Ok(relative)
}

fn resolve_worktree_file(workdir: &Path, relative: &str) -> Result<PathBuf, G4uError> {
    let relative = safe_relative_path(relative)?;
    let base = fs::canonicalize(workdir)
        .map_err(|err| G4uError::internal(format!("无法解析仓库目录: {err}")))?;
    let resolved = fs::canonicalize(base.join(relative)).map_err(|err| {
        G4uError::new(
            ErrorKind::CommandFailed,
            format!("文件不存在或不可读: {err}"),
        )
    })?;
    if !resolved.starts_with(&base) {
        return Err(G4uError::new(
            ErrorKind::CommandFailed,
            "文件路径超出仓库范围。",
        ));
    }
    Ok(resolved)
}

fn env_value(keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| std::env::var(key).ok().filter(|value| !value.is_empty()))
}

fn gitignore_path(session: &Session) -> Result<PathBuf, G4uError> {
    if session.git.is_bare {
        return Err(G4uError::new(
            ErrorKind::CommandFailed,
            "裸仓库没有工作区，无法创建 .gitignore。",
        ));
    }
    Ok(session.git.workdir.join(".gitignore"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {:?}: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[test]
    fn service_roundtrip_and_registry_persistence() {
        let root = std::env::temp_dir().join(format!(
            "g4u-ops-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let repo_dir = root.join("repo");
        let state_dir = root.join("state");
        fs::create_dir_all(&repo_dir).unwrap();

        git(&repo_dir, &["init", "-q"]);
        git(&repo_dir, &["config", "user.email", "test@example.com"]);
        git(&repo_dir, &["config", "user.name", "Test"]);

        let service = RepoService::new(state_dir.clone());
        let summary = service.open(repo_dir.to_str().unwrap()).unwrap();
        assert!(summary.valid);
        assert_eq!(service.list().len(), 1);

        fs::write(repo_dir.join("a.txt"), "a\n").unwrap();
        let status = service.status(&summary.id).unwrap();
        assert_eq!(status.entries.len(), 1);
        assert_eq!(
            status.entries[0].worktree_state,
            g4u_model::ChangeKind::Untracked
        );
        assert!(status.version >= 1);

        service.stage(&summary.id, &[]).unwrap();
        assert_eq!(service.status(&summary.id).unwrap().staged_count(), 1);

        let oid = service
            .commit(&summary.id, "init: a\n", false, true)
            .unwrap();
        let stats = service.list().pop().unwrap().stats.unwrap();
        assert_eq!(stats.commit_count, 1);
        assert_eq!(stats.branch_count, 1);
        assert!(stats.last_commit_summary.is_some());
        assert_eq!(service.log(&summary.id, 10).unwrap().len(), 1);
        assert_eq!(
            service
                .commit_detail(&summary.id, &oid)
                .unwrap()
                .files
                .len(),
            1
        );
        let listing = service.branches(&summary.id).unwrap();
        assert!(listing.branches.iter().any(|b| b.is_current));

        let info = service.app_info("0.1.0");
        assert_eq!(info.app_version, "0.1.0");
        assert_eq!(info.engine, "系统 git CLI");
        assert!(!info.git_version.is_empty());
        assert!(!info.app_data_dir.is_empty());

        // 新进程按持久化路径重建会话。
        let reopened = RepoService::new(state_dir);
        assert_eq!(reopened.list().len(), 1);
        assert!(reopened.status(&summary.id).is_ok());

        reopened.remove(&summary.id).unwrap();
        assert!(reopened.list().is_empty());

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn commit_and_amend_undo_keep_changes_staged() {
        let root = std::env::temp_dir().join(format!(
            "g4u-undo-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let repo_dir = root.join("repo");
        let state_dir = root.join("state");
        fs::create_dir_all(&repo_dir).unwrap();
        git(&repo_dir, &["init", "-q"]);
        git(&repo_dir, &["config", "user.email", "test@example.com"]);
        git(&repo_dir, &["config", "user.name", "Test"]);

        let service = RepoService::new(state_dir.clone());
        let summary = service.open(repo_dir.to_str().unwrap()).unwrap();
        let id = summary.id.clone();

        fs::write(repo_dir.join("a.txt"), "one\n").unwrap();
        service.stage(&id, &[]).unwrap();
        let first = service.commit(&id, "one\n", false, true).unwrap();

        fs::write(repo_dir.join("a.txt"), "two\n").unwrap();
        service.stage(&id, &[]).unwrap();
        let second = service.commit(&id, "two\n", false, true).unwrap();

        assert_eq!(
            service.undo_last(&id, &second).unwrap(),
            Some(first.clone())
        );
        assert_eq!(service.log(&id, 10).unwrap()[0].oid, first);
        assert_eq!(service.status(&id).unwrap().staged_count(), 1);
        // 已撤销过的记录不再可撤销。
        assert_eq!(service.undo_last(&id, &second).unwrap(), None);

        fs::write(repo_dir.join("a.txt"), "three\n").unwrap();
        service.stage(&id, &[]).unwrap();
        let amended = service.commit(&id, "three\n", true, true).unwrap();
        assert_eq!(
            service.undo_last(&id, &amended).unwrap(),
            Some(first.clone())
        );
        assert_eq!(service.log(&id, 10).unwrap()[0].oid, first);
        assert_eq!(service.status(&id).unwrap().staged_count(), 1);

        let refs = Command::new("git")
            .arg("-C")
            .arg(&repo_dir)
            .args(["for-each-ref", "--format=%(refname)", "refs/git4u/backup/"])
            .output()
            .unwrap();
        let refs = String::from_utf8_lossy(&refs.stdout);
        assert!(refs.lines().count() >= 2, "{refs}");

        let journal = fs::read_to_string(state_dir.join("journal.jsonl")).unwrap();
        assert!(journal.contains("\"op\":\"commit.create\""), "{journal}");
        assert!(journal.contains("\"op\":\"commit.amend\""), "{journal}");
        assert!(journal.contains("\"op\":\"undo\""), "{journal}");
        assert!(!service.backup_refs(&id).unwrap().is_empty());

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn tool_preview_and_large_files_stay_in_worktree() {
        let root = std::env::temp_dir().join(format!(
            "g4u-tools-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let repo_dir = root.join("repo");
        let state_dir = root.join("state");
        fs::create_dir_all(repo_dir.join("src")).unwrap();
        git(&repo_dir, &["init", "-q"]);
        git(&repo_dir, &["config", "user.email", "test@example.com"]);
        git(&repo_dir, &["config", "user.name", "Test"]);
        fs::write(repo_dir.join("src/a.txt"), "hello\n").unwrap();
        fs::write(repo_dir.join("big.bin"), vec![b'x'; 4096]).unwrap();
        fs::write(repo_dir.join("binary.bin"), [0u8, 1, 2, 3]).unwrap();
        git(&repo_dir, &["add", "."]);
        git(&repo_dir, &["commit", "-q", "-m", "init"]);

        let service = RepoService::new(state_dir);
        let summary = service.open(repo_dir.to_str().unwrap()).unwrap();
        let id = summary.id;

        let files = service.files(&id).unwrap();
        assert!(files.contains(&"src/a.txt".to_string()));

        let preview = service.file_preview(&id, "src/a.txt").unwrap();
        assert_eq!(preview.content, "hello\n");
        assert!(!preview.binary);
        assert!(!preview.truncated);

        let binary = service.file_preview(&id, "binary.bin").unwrap();
        assert!(binary.binary);
        assert!(binary.content.is_empty());

        assert!(service.file_preview(&id, "../outside.txt").is_err());
        assert!(service.file_preview(&id, "/etc/passwd").is_err());
        assert!(service.file_preview(&id, ".git/config").is_err());

        let large = service.large_files(&id, 10).unwrap();
        assert_eq!(large[0].path, "big.bin");
        assert!(large[0].size >= 4096);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn diagnostics_are_read_only_and_report_findings() {
        let root = std::env::temp_dir().join(format!(
            "g4u-diag-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let repo_dir = root.join("repo");
        let state_dir = root.join("state");
        fs::create_dir_all(&repo_dir).unwrap();
        git(&repo_dir, &["init", "-q"]);
        git(&repo_dir, &["config", "user.email", "test@example.com"]);
        git(&repo_dir, &["config", "user.name", "Test"]);
        git(&repo_dir, &["config", "core.autocrlf", "false"]);
        fs::write(repo_dir.join(".gitignore"), "ignored.txt\n").unwrap();
        fs::write(repo_dir.join("ignored.txt"), "ignored\n").unwrap();
        fs::write(repo_dir.join("mixed.txt"), b"a\r\nb\n").unwrap();
        fs::write(
            repo_dir.join("secret.txt"),
            "token = \"AKIA1234567890ABCDEF\"\n",
        )
        .unwrap();
        git(&repo_dir, &["add", "."]);
        git(&repo_dir, &["commit", "-q", "-m", "init"]);

        let service = RepoService::new(state_dir);
        let summary = service.open(repo_dir.to_str().unwrap()).unwrap();
        let id = summary.id;

        let ignore = service.ignore_check(&id, "ignored.txt").unwrap().unwrap();
        assert_eq!(ignore.pattern, "ignored.txt");
        assert!(service.ignore_check(&id, "mixed.txt").unwrap().is_none());
        assert!(service.ignore_check(&id, "../outside").is_err());

        let eol = service.eol_report(&id).unwrap();
        assert!(eol.mixed >= 1, "{eol:?}");

        let stats = service.object_stats(&id).unwrap();
        assert!(stats.count + stats.in_pack > 0);

        let reflog = service.reflog(&id, 10).unwrap();
        assert!(!reflog.is_empty());

        let locks = service.lock_report(&id).unwrap();
        assert!(locks.index_lock.is_none());

        let env = service.env_report(&id).unwrap();
        assert!(!env.git_version.is_empty());
        assert!(!env.workdir.is_empty());

        let secrets = service.secret_scan(&id, 100).unwrap();
        assert!(secrets.iter().any(|item| item.pattern == "awsKey"));

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn rejects_non_repository_paths() {
        let root = std::env::temp_dir().join(format!("g4u-ops-bad-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let service = RepoService::new(root.join("state"));
        let err = service.open(root.to_str().unwrap()).unwrap_err();
        assert_eq!(err.kind, g4u_model::ErrorKind::NotARepo);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn gitignore_templates_append_and_take_effect() {
        let root = std::env::temp_dir().join(format!(
            "g4u-ops-ignore-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let repo_dir = root.join("repo");
        fs::create_dir_all(&repo_dir).unwrap();
        git(&repo_dir, &["init", "-q"]);

        let service = RepoService::new(root.join("state"));
        let summary = service.open(repo_dir.to_str().unwrap()).unwrap();

        let initial = service.gitignore(&summary.id).unwrap();
        assert!(!initial.exists);
        assert!(initial.content.is_empty());

        service
            .write_gitignore(&summary.id, "# Rust\ntarget/\n", false)
            .unwrap();
        service
            .write_gitignore(&summary.id, "# Logs\n*.log\n", true)
            .unwrap();

        let written = service.gitignore(&summary.id).unwrap();
        assert!(written.exists);
        assert!(written.content.contains("target/"));
        assert!(written.content.contains("*.log"));

        fs::write(repo_dir.join("app.log"), "noise\n").unwrap();
        let status = service.status(&summary.id).unwrap();
        assert!(!status.entries.iter().any(|e| e.path == "app.log"));

        assert!(service.write_gitignore(&summary.id, "   ", false).is_err());

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn settings_scope_persist_and_validate() {
        let root = std::env::temp_dir().join(format!(
            "g4u-ops-settings-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let repo_dir = root.join("repo");
        let state_dir = root.join("state");
        fs::create_dir_all(&repo_dir).unwrap();
        git(&repo_dir, &["init", "-q"]);

        let service = RepoService::new(state_dir.clone());
        let summary = service.open(repo_dir.to_str().unwrap()).unwrap();

        let initial = service.settings(None);
        assert_eq!(initial.global.pull_strategy, g4u_model::PullStrategy::Merge);
        assert!(initial.repo.is_none());

        let mut global = initial.global.clone();
        global.protected_branches = "^(main|release/.*)$".into();
        service
            .save_settings(g4u_model::SettingsScope::Global, None, global.clone())
            .unwrap();

        let mut repo_settings = global.clone();
        repo_settings.pull_strategy = g4u_model::PullStrategy::Rebase;
        let saved = service
            .save_settings(
                g4u_model::SettingsScope::Repo,
                Some(&summary.id),
                repo_settings.clone(),
            )
            .unwrap();
        assert_eq!(saved.repo, Some(repo_settings.clone()));
        assert_eq!(
            saved.effective.pull_strategy,
            g4u_model::PullStrategy::Rebase
        );

        let reopened = RepoService::new(state_dir);
        let persisted = reopened.settings(Some(&summary.id));
        assert_eq!(persisted.repo, Some(repo_settings.clone()));

        let cleared = reopened
            .save_settings(g4u_model::SettingsScope::Repo, Some(&summary.id), global)
            .unwrap();
        assert!(cleared.repo.is_none());

        let mut invalid = cleared.global.clone();
        invalid.protected_branches = "([".into();
        assert!(reopened
            .save_settings(g4u_model::SettingsScope::Global, None, invalid)
            .is_err());

        let reset = reopened.reset_settings().unwrap();
        assert_eq!(reset.global.pull_strategy, g4u_model::PullStrategy::Merge);
        assert!(reset.repo.is_none());

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn cleanup_old_backup_refs_by_name_timestamp() {
        let root = std::env::temp_dir().join(format!(
            "g4u-ops-backup-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let repo_dir = root.join("repo");
        fs::create_dir_all(&repo_dir).unwrap();
        git(&repo_dir, &["init", "-q"]);
        git(&repo_dir, &["config", "user.email", "test@example.com"]);
        git(&repo_dir, &["config", "user.name", "Test"]);
        fs::write(repo_dir.join("a.txt"), "a\n").unwrap();
        git(&repo_dir, &["add", "."]);
        git(&repo_dir, &["commit", "-q", "-m", "init"]);
        git(
            &repo_dir,
            &[
                "update-ref",
                "refs/git4u/backup/20200101-000000-test",
                "HEAD",
            ],
        );

        let service = RepoService::new(root.join("state"));
        let summary = service.open(repo_dir.to_str().unwrap()).unwrap();
        assert_eq!(service.cleanup_backups(&summary.id).unwrap(), 1);
        let refs = Command::new("git")
            .arg("-C")
            .arg(&repo_dir)
            .args(["show-ref"])
            .output()
            .unwrap();
        assert!(!String::from_utf8_lossy(&refs.stdout).contains("refs/git4u/backup/"));

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn proxy_crud_and_settings_references() {
        let root = std::env::temp_dir().join(format!(
            "g4u-ops-proxy-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let repo_dir = root.join("repo");
        let state_dir = root.join("state");
        fs::create_dir_all(&repo_dir).unwrap();
        git(&repo_dir, &["init", "-q"]);

        let service = RepoService::new(state_dir.clone());
        let summary = service.open(repo_dir.to_str().unwrap()).unwrap();

        let proxies = service
            .save_proxy(g4u_model::ProxyInput {
                id: None,
                name: "公司代理".into(),
                url: "http://proxy.company.com:8080".into(),
            })
            .unwrap();
        assert_eq!(proxies.len(), 1);
        let proxy_id = proxies[0].id.clone();

        let mut global = service.settings(None).global;
        global.proxy_id = Some(proxy_id.clone());
        let saved = service
            .save_settings(g4u_model::SettingsScope::Global, None, global.clone())
            .unwrap();
        assert_eq!(saved.effective.proxy_id.as_deref(), Some(proxy_id.as_str()));

        let mut repo_settings = global.clone();
        repo_settings.proxy_id = None;
        let saved = service
            .save_settings(
                g4u_model::SettingsScope::Repo,
                Some(&summary.id),
                repo_settings,
            )
            .unwrap();
        assert!(saved.effective.proxy_id.is_none());

        service.delete_proxy(&proxy_id).unwrap();
        let after = service.settings(Some(&summary.id));
        assert!(after.proxies.is_empty());
        assert!(after.global.proxy_id.is_none());
        assert!(after.effective.proxy_id.is_none());

        assert!(service
            .save_proxy(g4u_model::ProxyInput {
                id: None,
                name: "bad".into(),
                url: "ftp://proxy".into(),
            })
            .is_err());

        let reopened = RepoService::new(state_dir);
        assert!(reopened.settings(Some(&summary.id)).proxies.is_empty());

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn sync_push_fetch_pull_and_audit() {
        let root = std::env::temp_dir().join(format!(
            "g4u-ops-sync-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let remote_dir = root.join("remote.git");
        let repo_dir = root.join("repo");
        let other_dir = root.join("other");
        fs::create_dir_all(&repo_dir).unwrap();
        fs::create_dir_all(&remote_dir).unwrap();

        git(&remote_dir, &["init", "--bare", "-q", "-b", "main"]);
        git(&repo_dir, &["init", "-q", "-b", "main"]);
        git(&repo_dir, &["config", "user.email", "test@example.com"]);
        git(&repo_dir, &["config", "user.name", "Test"]);
        fs::write(repo_dir.join("a.txt"), "a\n").unwrap();
        git(&repo_dir, &["add", "."]);
        git(&repo_dir, &["commit", "-q", "-m", "init"]);
        git(
            &repo_dir,
            &["remote", "add", "origin", remote_dir.to_str().unwrap()],
        );

        let service = RepoService::new(root.join("state"));
        let summary = service.open(repo_dir.to_str().unwrap()).unwrap();

        let audit = service.push_audit(&summary.id).unwrap();
        assert!(audit
            .blockers
            .iter()
            .any(|item| item.kind == "protectedBranch"));
        assert!(service
            .push(&summary.id, Some("origin"), true, false)
            .is_err());
        service
            .push(&summary.id, Some("origin"), true, true)
            .unwrap();

        git(
            &root,
            &[
                "clone",
                "-q",
                remote_dir.to_str().unwrap(),
                other_dir.to_str().unwrap(),
            ],
        );
        git(&other_dir, &["config", "user.email", "other@example.com"]);
        git(&other_dir, &["config", "user.name", "Other"]);
        fs::write(other_dir.join("b.txt"), "b\n").unwrap();
        git(&other_dir, &["add", "."]);
        git(&other_dir, &["commit", "-q", "-m", "remote change"]);
        git(&other_dir, &["push", "-q", "origin", "main"]);

        service.fetch(&summary.id, None).unwrap();
        let behind = service
            .status(&summary.id)
            .unwrap()
            .upstream
            .unwrap()
            .behind;
        assert!(behind >= 1);
        service.pull(&summary.id).unwrap();
        assert!(repo_dir.join("b.txt").exists());

        fs::write(
            repo_dir.join("secret.txt"),
            "token = \"AKIA1234567890ABCDEF\"\n",
        )
        .unwrap();
        git(&repo_dir, &["add", "."]);
        git(&repo_dir, &["commit", "-q", "-m", "add secret"]);
        let audit = service.push_audit(&summary.id).unwrap();
        assert!(audit.blockers.iter().any(|item| item.kind == "awsKey"));
        assert!(service
            .push(&summary.id, Some("origin"), false, false)
            .is_err());

        fs::remove_dir_all(&root).ok();
    }
}
