//! g4u-model — 领域模型与 IPC 契约（纯数据，无 IO）。

use serde::{Deserialize, Serialize};

pub type RepoId = String;
pub type Oid = String;

// ---------------------------------------------------------------------------
// 错误模型（02-6 四件套：kind / human / suggestions / cli）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorKind {
    NotARepo,
    GitNotFound,
    NoSuchRepo,
    IndexLocked,
    IdentityMissing,
    AuthFailed,
    CheckoutConflict,
    MidStateBlocking,
    NothingToCommit,
    EmptyMessage,
    CommandFailed,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Risk {
    Safe,
    Caution,
    Dangerous,
}

/// 结构化建议，`action` 是前端可绑定的 ActionId（如 `repo.refresh`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub label: String,
    pub risk: Risk,
    pub action: String,
    /// 动作的可选目标，例如要更新的凭证 id；前端按 action 解释。
    #[serde(default)]
    pub target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct G4uError {
    pub kind: ErrorKind,
    pub human: String,
    pub cause_chain: Vec<String>,
    pub suggestions: Vec<Suggestion>,
    pub cli_equivalent: Option<String>,
}

impl G4uError {
    pub fn new(kind: ErrorKind, human: impl Into<String>) -> Self {
        Self {
            kind,
            human: human.into(),
            cause_chain: Vec::new(),
            suggestions: Vec::new(),
            cli_equivalent: None,
        }
    }

    pub fn internal(cause: impl Into<String>) -> Self {
        Self::new(ErrorKind::Internal, "内部错误，请重试或查看诊断日志").with_cause(cause)
    }

    pub fn with_cause(mut self, cause: impl Into<String>) -> Self {
        self.cause_chain.push(cause.into());
        self
    }

    pub fn with_cli(mut self, cli: impl Into<String>) -> Self {
        self.cli_equivalent = Some(cli.into());
        self
    }

    pub fn with_suggestions(mut self, suggestions: Vec<Suggestion>) -> Self {
        self.suggestions = suggestions;
        self
    }
}

pub fn suggest(label: impl Into<String>, risk: Risk, action: impl Into<String>) -> Suggestion {
    Suggestion {
        label: label.into(),
        risk,
        action: action.into(),
        target: None,
    }
}

/// 带目标的结构化建议，例如 `credentials.update` + 凭证 id。
pub fn suggest_target(
    label: impl Into<String>,
    risk: Risk,
    action: impl Into<String>,
    target: impl Into<String>,
) -> Suggestion {
    Suggestion {
        label: label.into(),
        risk,
        action: action.into(),
        target: Some(target.into()),
    }
}

// ---------------------------------------------------------------------------
// 仓库 / 状态
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HeadKind {
    Branch,
    Detached,
    Unborn,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadState {
    pub kind: HeadKind,
    pub branch: Option<String>,
    pub oid: Option<Oid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangeKind {
    Unmodified,
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    Untracked,
    Unmerged,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusEntry {
    pub path: String,
    pub orig_path: Option<String>,
    pub index_state: ChangeKind,
    pub worktree_state: ChangeKind,
    /// 索引与 HEAD 有差异（会进入下一次提交）。
    pub staged: bool,
    pub additions: u32,
    pub deletions: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamInfo {
    pub name: String,
    pub ahead: u32,
    pub behind: u32,
    pub gone: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoStatus {
    pub version: u64,
    pub head: HeadState,
    pub upstream: Option<UpstreamInfo>,
    pub entries: Vec<StatusEntry>,
    pub conflicts: Vec<String>,
    pub mid_state: Option<String>,
}

/// 仓库根目录 `.gitignore` 的读取结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitignoreFile {
    pub exists: bool,
    pub content: String,
}

/// 只读文件预览；大文件截断，二进制返回标记而不返回内容。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePreview {
    pub path: String,
    pub size: u64,
    pub binary: bool,
    pub truncated: bool,
    pub content: String,
}

/// `refs/git4u/backup/*` 中的一条恢复点。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupRef {
    pub name: String,
    pub oid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LargeFile {
    pub path: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IgnoreMatch {
    pub source: String,
    pub line: u32,
    pub pattern: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EolFile {
    pub path: String,
    pub index: String,
    pub worktree: String,
    pub attr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EolReport {
    pub total: usize,
    pub lf: usize,
    pub crlf: usize,
    pub mixed: usize,
    pub files: Vec<EolFile>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectStats {
    pub count: u64,
    pub size_kb: u64,
    pub in_pack: u64,
    pub size_pack_kb: u64,
    pub packs: u64,
    pub prune_packable: u64,
    pub garbage: u64,
    pub size_garbage_kb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReflogEntry {
    pub oid: String,
    pub selector: String,
    pub subject: String,
    pub at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockInfo {
    pub modified_at: u64,
    pub age_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockReport {
    pub index_lock: Option<LockInfo>,
    pub mid_state: Option<String>,
    pub state_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvReport {
    pub git_version: String,
    pub workdir: String,
    pub git_dir: String,
    pub hooks_path: Option<String>,
    pub credential_helper: Option<String>,
    pub http_proxy: Option<String>,
    pub https_proxy: Option<String>,
    pub no_proxy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretFinding {
    pub path: String,
    pub line: usize,
    pub pattern: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum PullStrategy {
    #[default]
    Merge,
    Rebase,
    FfOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoSettings {
    pub pull_strategy: PullStrategy,
    /// 正则；命中当前分支时危险操作需要二次确认。
    pub protected_branches: String,
    pub push_audit: bool,
    pub secret_scan: bool,
    /// null = 不使用代理；Some(id) 引用全局代理列表。
    pub proxy_id: Option<String>,
    pub auto_fetch: bool,
    pub auto_fetch_minutes: u32,
    pub backup_retention_days: u32,
}

impl Default for RepoSettings {
    fn default() -> Self {
        Self {
            pull_strategy: PullStrategy::Merge,
            protected_branches: "^(main|master|develop|release/.*)$".into(),
            push_audit: true,
            secret_scan: true,
            proxy_id: None,
            auto_fetch: false,
            auto_fetch_minutes: 5,
            backup_retention_days: 30,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyEntry {
    pub id: String,
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyInput {
    pub id: Option<String>,
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SettingsScope {
    Global,
    Repo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub global: RepoSettings,
    /// 当前仓库的覆盖值；未覆盖时为 null。
    pub repo: Option<RepoSettings>,
    pub effective: RepoSettings,
    pub proxies: Vec<ProxyEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteInfo {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CredentialMode {
    Password,
    Token,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialEntry {
    pub id: String,
    pub url: String,
    pub mode: CredentialMode,
    pub username: String,
    pub last_used: Option<u64>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialInput {
    pub id: Option<String>,
    pub url: String,
    pub mode: CredentialMode,
    pub username: String,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialValidation {
    pub ok: bool,
    pub message: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditFinding {
    pub kind: String,
    pub path: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PushAuditReport {
    pub branch: String,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub blockers: Vec<AuditFinding>,
    pub warnings: Vec<AuditFinding>,
}

impl PushAuditReport {
    pub fn can_push(&self) -> bool {
        self.blockers.is_empty()
    }
}

impl RepoStatus {
    pub fn staged_count(&self) -> usize {
        self.entries.iter().filter(|e| e.staged).count()
    }

    pub fn unstaged_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| !e.staged && e.worktree_state != ChangeKind::Unmodified)
            .count()
    }
}

/// 仪表盘卡片上的仓库体积与历史统计。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoStats {
    /// Git 对象存储（.git）占用，字节；不含工作区文件。
    pub size_bytes: u64,
    /// 当前 HEAD 可达的提交数；空仓库为 0。
    pub commit_count: u64,
    pub branch_count: usize,
    pub tag_count: usize,
    pub last_commit_at: Option<i64>,
    pub last_commit_summary: Option<String>,
}

/// 仪表盘 / 侧栏用的轻量仓库摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoSummary {
    pub id: RepoId,
    pub name: String,
    pub path: String,
    pub valid: bool,
    pub head: Option<HeadState>,
    pub staged: usize,
    pub unstaged: usize,
    pub conflicts: usize,
    pub ahead: u32,
    pub behind: u32,
    pub mid_state: Option<String>,
    /// 体积/历史统计；路径失效时为 None。
    pub stats: Option<RepoStats>,
    /// 打开失败的仓库（路径失效等）保留在列表中并标注原因。
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub app_version: String,
    pub git_version: String,
    pub engine: String,
    pub app_data_dir: String,
}

// ---------------------------------------------------------------------------
// 历史 / 分支
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitSummary {
    pub oid: Oid,
    pub short_oid: String,
    pub parents: Vec<Oid>,
    pub summary: String,
    pub author_name: String,
    pub author_email: String,
    pub committed_at: i64,
    pub refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStat {
    pub path: String,
    pub change: ChangeKind,
    pub additions: u32,
    pub deletions: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitDetail {
    pub commit: CommitSummary,
    pub body: String,
    pub files: Vec<FileStat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchInfo {
    pub name: String,
    pub full_name: String,
    pub oid: Oid,
    pub is_current: bool,
    pub is_remote: bool,
    pub remote: Option<String>,
    pub upstream: Option<String>,
    pub track: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub gone: bool,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagInfo {
    pub name: String,
    pub oid: Oid,
    pub subject: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchListing {
    pub branches: Vec<BranchInfo>,
    pub tags: Vec<TagInfo>,
}

// ---------------------------------------------------------------------------
// Diff
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiffLineKind {
    Context,
    Add,
    Del,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffHunk {
    pub header: String,
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub path: String,
    pub old_path: Option<String>,
    pub binary: bool,
    pub additions: u32,
    pub deletions: u32,
    pub hunks: Vec<DiffHunk>,
}

impl FileDiff {
    pub fn empty(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            old_path: None,
            binary: false,
            additions: 0,
            deletions: 0,
            hunks: Vec::new(),
        }
    }
}
