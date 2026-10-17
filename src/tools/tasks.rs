//! Tasks store and Productivity tools (S4-T3).
//!
//! One `tasks` table (migration 008) shared by Work Productivity and Home Personal Productivity,
//! separated by the domain column (ADR-002). Deadlines, focus sessions, reminders, errands and
//! household chores are tasks with a `kind`. Postponing a task increments `postponed_count`;
//! the Balance Agent reads that as postponement debt (§9.1) and the ledger records a
//! content-free `task_postponed` event so the pattern layer (§7.2) can count it.
//!
//! The tools (`create_task`, `list_tasks`, `postpone_task`, `complete_task`, `plan_day`) are
//! served in-process — there is no MCP child. An agent receives them when its allowlist has an
//! entry with `mcp_server = "tasks"`, and they pass through the permission gate like every other
//! tool (`write_local` for changes, `read` for lists and the day plan). Times are UTC until the
//! profile timezone lands in memory (S3 follow-up).

use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use adk_tool::FunctionTool;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::domain::Domain;
use crate::intelligence::ledger::ActivityEvent;
use crate::permissions::Effect;

/// The pseudo MCP server id used in `mcp_allowlists.toml` for this built-in toolset.
pub const TOOLSET_ID: &str = "tasks";
pub const TOOL_NAMES: [&str; 5] = ["create_task", "list_tasks", "postpone_task", "complete_task", "plan_day"];

const MAX_TITLE: usize = 200;
const MAX_NOTES: usize = 500;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    #[default]
    Task,
    Deadline,
    FocusSession,
    Reminder,
    Errand,
    Household,
}

impl TaskKind {
    pub const ALL: [TaskKind; 6] = [
        TaskKind::Task,
        TaskKind::Deadline,
        TaskKind::FocusSession,
        TaskKind::Reminder,
        TaskKind::Errand,
        TaskKind::Household,
    ];
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskKind::Task => "task",
            TaskKind::Deadline => "deadline",
            TaskKind::FocusSession => "focus_session",
            TaskKind::Reminder => "reminder",
            TaskKind::Errand => "errand",
            TaskKind::Household => "household",
        }
    }
    pub fn parse(s: &str) -> Option<TaskKind> {
        let s = s.trim().to_ascii_lowercase();
        Self::ALL.iter().copied().find(|k| k.as_str() == s)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Low,
    #[default]
    Normal,
    High,
}

impl Priority {
    pub fn as_str(&self) -> &'static str {
        match self {
            Priority::Low => "low",
            Priority::Normal => "normal",
            Priority::High => "high",
        }
    }
    pub fn parse(s: &str) -> Option<Priority> {
        match s.trim().to_ascii_lowercase().as_str() {
            "low" => Some(Priority::Low),
            "normal" | "medium" => Some(Priority::Normal),
            "high" | "urgent" => Some(Priority::High),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    #[default]
    Open,
    Done,
    Cancelled,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::Open => "open",
            TaskStatus::Done => "done",
            TaskStatus::Cancelled => "cancelled",
        }
    }
    pub fn parse(s: &str) -> Option<TaskStatus> {
        match s.trim().to_ascii_lowercase().as_str() {
            "open" => Some(TaskStatus::Open),
            "done" | "completed" => Some(TaskStatus::Done),
            "cancelled" | "canceled" => Some(TaskStatus::Cancelled),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub user_id: String,
    pub domain: Domain,
    pub title: String,
    pub kind: TaskKind,
    pub due: Option<DateTime<Utc>>,
    pub duration_minutes: Option<i32>,
    pub priority: Priority,
    pub status: TaskStatus,
    pub postponed_count: i32,
    pub source_agent: String,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

pub struct NewTask<'a> {
    pub domain: Domain,
    pub title: &'a str,
    pub kind: TaskKind,
    pub due: Option<DateTime<Utc>>,
    pub duration_minutes: Option<i32>,
    pub priority: Priority,
    pub source_agent: &'a str,
    pub notes: Option<&'a str>,
}

/// Filters for [`TaskStore::list`]. Due-window filters exclude undated tasks.
#[derive(Clone, Debug, Default)]
pub struct TaskFilter {
    pub domain: Option<Domain>,
    pub status: Option<TaskStatus>,
    pub kind: Option<TaskKind>,
    pub due_before: Option<DateTime<Utc>>,
    pub due_after: Option<DateTime<Utc>>,
}

impl TaskFilter {
    fn matches(&self, t: &Task) -> bool {
        self.domain.map(|d| t.domain == d).unwrap_or(true)
            && self.status.map(|s| t.status == s).unwrap_or(true)
            && self.kind.map(|k| t.kind == k).unwrap_or(true)
            && match (self.due_before, self.due_after) {
                (None, None) => true,
                (before, after) => t
                    .due
                    .map(|d| before.map(|b| d <= b).unwrap_or(true) && after.map(|a| d >= a).unwrap_or(true))
                    .unwrap_or(false),
            }
    }
}

/// What an agent in `scope` may see: its own world plus `shared`; shared agents see everything.
pub fn visible(scope: Domain, task_domain: Domain) -> bool {
    scope == Domain::Shared || task_domain == scope || task_domain == Domain::Shared
}

/// Which domain a write lands in: shared agents choose (default `shared`); world agents may only
/// write their own world or `shared`.
pub fn write_domain(scope: Domain, requested: Option<Domain>) -> Result<Domain, String> {
    match (scope, requested) {
        (Domain::Shared, r) => Ok(r.unwrap_or(Domain::Shared)),
        (scope, None) => Ok(scope),
        (scope, Some(r)) if r == scope || r == Domain::Shared => Ok(r),
        (scope, Some(r)) => Err(format!("an agent in the {scope} world may not create {r} tasks")),
    }
}

pub fn start_of_day(d: NaiveDate) -> DateTime<Utc> {
    d.and_hms_opt(0, 0, 0).expect("valid time").and_utc()
}

pub fn end_of_day(d: NaiveDate) -> DateTime<Utc> {
    d.and_hms_opt(23, 59, 59).expect("valid time").and_utc()
}

/// `null`/missing → none; RFC 3339 → exact; `YYYY-MM-DD`, `today`, `tomorrow` → end of that day (UTC).
pub fn parse_due(value: Option<&serde_json::Value>, now: DateTime<Utc>) -> Result<Option<DateTime<Utc>>, String> {
    let Some(v) = value else { return Ok(None) };
    if v.is_null() {
        return Ok(None);
    }
    let Some(s) = v.as_str() else {
        return Err("due must be a string: RFC 3339, YYYY-MM-DD, today or tomorrow".into());
    };
    let s = s.trim();
    if s.is_empty() {
        return Ok(None);
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(Some(dt.with_timezone(&Utc)));
    }
    let date = match s.to_ascii_lowercase().as_str() {
        "today" => now.date_naive(),
        "tomorrow" => now.date_naive() + Duration::days(1),
        other => NaiveDate::parse_from_str(other, "%Y-%m-%d")
            .map_err(|_| format!("could not parse due '{s}' — use RFC 3339, YYYY-MM-DD, today or tomorrow"))?,
    };
    Ok(Some(end_of_day(date)))
}

fn sort_tasks(tasks: &mut [Task]) {
    tasks.sort_by(|a, b| {
        match (a.due, b.due) {
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        }
        .then_with(|| b.priority.cmp(&a.priority))
        .then_with(|| a.created_at.cmp(&b.created_at))
    });
}

/// Deterministic day plan — no LLM involved (§3.1 "maintain unified priorities").
#[derive(Clone, Debug, Serialize)]
pub struct DayPlan {
    pub date: NaiveDate,
    pub timezone: &'static str,
    /// Open tasks due before the day started.
    pub overdue: Vec<Task>,
    /// Open tasks due during the day (focus sessions listed separately).
    pub due_today: Vec<Task>,
    pub focus_sessions: Vec<Task>,
    /// Due in the three days after the day.
    pub upcoming: Vec<Task>,
    /// Open tasks postponed three or more times (§9.1 postponement debt).
    pub postponed_debt: Vec<Task>,
    pub open_total: usize,
}

#[derive(Default)]
struct Inner {
    users: HashMap<String, Vec<Task>>,
    loads: crate::db::UserLoads,
}

#[derive(Clone)]
pub struct TaskStore {
    inner: Arc<RwLock<Inner>>,
    pg: Option<PgPool>,
}

impl TaskStore {
    pub fn new(pg: Option<PgPool>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner::default())),
            pg,
        }
    }

    pub fn in_memory() -> Self {
        Self::new(None)
    }

    pub fn postgres_enabled(&self) -> bool {
        self.pg.is_some()
    }

    /// Load the user's tasks on first touch; a database error leaves the user unloaded so a later
    /// touch reads again. Tasks written here meanwhile are newer than their stored rows and win.
    async fn ensure_loaded(&self, user_id: &str) {
        let Some(pool) = &self.pg else { return };
        if !self.inner.read().await.loads.should_load(user_id) {
            return;
        }
        match load_pg(pool, user_id).await {
            Ok(mut rows) => {
                let mut guard = self.inner.write().await;
                if guard.loads.is_loaded(user_id) {
                    return;
                }
                for task in guard.users.remove(user_id).unwrap_or_default() {
                    rows.retain(|r| r.id != task.id);
                    rows.push(task);
                }
                guard.users.insert(user_id.to_string(), rows);
                guard.loads.succeeded(user_id);
            }
            Err(e) => {
                tracing::warn!("tasks load failed; retrying on a later request: {e:#}");
                self.inner.write().await.loads.failed(user_id);
            }
        }
    }

    async fn persist(&self, task: &Task) {
        let Some(pool) = &self.pg else { return };
        if let Err(e) = upsert_pg(pool, task).await {
            tracing::warn!("tasks persist failed: {e:#}");
        }
    }

    /// Content-free ledger row: kind, domain, hashed task id, counts. Never the title.
    fn ledger(&self, task: &Task, kind: &str, agent: &str) {
        let svc = crate::permissions::gate::services();
        let ev = ActivityEvent::new(task.user_id.clone(), task.domain, agent, kind)
            .effect(Effect::WriteLocal)
            .subject(svc.ledger.hash_key(), &task.id.to_string())
            .meta(serde_json::json!({
                "kind": task.kind.as_str(),
                "postponed_count": task.postponed_count,
            }));
        svc.ledger.record(ev);
    }

    pub async fn create(&self, user_id: &str, new: NewTask<'_>) -> Task {
        self.ensure_loaded(user_id).await;
        let now = Utc::now();
        let task = Task {
            id: Uuid::new_v4(),
            user_id: user_id.to_string(),
            domain: new.domain,
            title: new.title.trim().chars().take(MAX_TITLE).collect(),
            kind: new.kind,
            due: new.due,
            duration_minutes: new.duration_minutes,
            priority: new.priority,
            status: TaskStatus::Open,
            postponed_count: 0,
            source_agent: new.source_agent.to_string(),
            notes: new.notes.map(|n| n.trim().chars().take(MAX_NOTES).collect::<String>()),
            created_at: now,
            updated_at: now,
            completed_at: None,
        };
        self.inner.write().await.users.entry(user_id.to_string()).or_default().push(task.clone());
        self.persist(&task).await;
        self.ledger(&task, "task_created", new.source_agent);
        task
    }

    pub async fn get(&self, user_id: &str, id: Uuid) -> Option<Task> {
        self.ensure_loaded(user_id).await;
        self.inner.read().await.users.get(user_id)?.iter().find(|t| t.id == id).cloned()
    }

    /// Tasks visible in `scope` matching `filter`, due-soonest first (undated last), then priority.
    pub async fn list(&self, user_id: &str, scope: Domain, filter: &TaskFilter) -> Vec<Task> {
        self.ensure_loaded(user_id).await;
        let mut out: Vec<Task> = self
            .inner
            .read()
            .await
            .users
            .get(user_id)
            .map(|v| v.iter().filter(|t| visible(scope, t.domain) && filter.matches(t)).cloned().collect())
            .unwrap_or_default();
        sort_tasks(&mut out);
        out
    }

    async fn mutate(&self, user_id: &str, id: Uuid, f: impl FnOnce(&mut Task)) -> Option<Task> {
        self.ensure_loaded(user_id).await;
        let task = {
            let mut guard = self.inner.write().await;
            let task = guard.users.get_mut(user_id)?.iter_mut().find(|t| t.id == id)?;
            f(task);
            task.updated_at = Utc::now();
            task.clone()
        };
        self.persist(&task).await;
        Some(task)
    }

    /// Move an open task's due date (default: one day later, or tomorrow when undated) and count
    /// the postponement. `None` when the task is missing or not open.
    pub async fn postpone(&self, user_id: &str, id: Uuid, due: Option<DateTime<Utc>>, agent: &str) -> Option<Task> {
        if self.get(user_id, id).await?.status != TaskStatus::Open {
            return None;
        }
        let task = self
            .mutate(user_id, id, |t| {
                t.postponed_count += 1;
                t.due = Some(due.unwrap_or_else(|| {
                    t.due
                        .map(|d| d + Duration::days(1))
                        .unwrap_or_else(|| end_of_day(Utc::now().date_naive() + Duration::days(1)))
                }));
            })
            .await?;
        self.ledger(&task, "task_postponed", agent);
        Some(task)
    }

    pub async fn complete(&self, user_id: &str, id: Uuid, agent: &str) -> Option<Task> {
        if self.get(user_id, id).await?.status != TaskStatus::Open {
            return None;
        }
        let task = self
            .mutate(user_id, id, |t| {
                t.status = TaskStatus::Done;
                t.completed_at = Some(Utc::now());
            })
            .await?;
        self.ledger(&task, "task_completed", agent);
        Some(task)
    }

    /// Group the open tasks visible in `scope` around `day` (UTC).
    /// Day plan over the open tasks `scope` can see, optionally narrowed to one `domain`.
    pub async fn plan_day(&self, user_id: &str, scope: Domain, domain: Option<Domain>, day: NaiveDate) -> DayPlan {
        let open = self
            .list(user_id, scope, &TaskFilter { domain, status: Some(TaskStatus::Open), ..Default::default() })
            .await;
        let start = start_of_day(day);
        let end = start + Duration::days(1);
        let horizon = end + Duration::days(3);
        let mut plan = DayPlan {
            date: day,
            timezone: "UTC",
            overdue: Vec::new(),
            due_today: Vec::new(),
            focus_sessions: Vec::new(),
            upcoming: Vec::new(),
            postponed_debt: Vec::new(),
            open_total: open.len(),
        };
        for t in open {
            if t.postponed_count >= 3 {
                plan.postponed_debt.push(t.clone());
            }
            match t.due {
                Some(d) if d < start => plan.overdue.push(t),
                Some(d) if d < end => {
                    if t.kind == TaskKind::FocusSession {
                        plan.focus_sessions.push(t)
                    } else {
                        plan.due_today.push(t)
                    }
                }
                Some(d) if d < horizon => plan.upcoming.push(t),
                _ => {}
            }
        }
        plan
    }

    /// Hard delete everything for the user (account deletion, S11).
    pub async fn purge(&self, user_id: &str) -> usize {
        self.ensure_loaded(user_id).await;
        let n = {
            let mut guard = self.inner.write().await;
            guard.users.remove(user_id).map(|v| v.len()).unwrap_or(0)
        };
        if let Some(pool) = &self.pg
            && let Err(e) = sqlx::query("DELETE FROM tasks WHERE user_id = $1").bind(user_id).execute(pool).await
        {
            tracing::warn!("tasks purge failed: {e:#}");
        }
        n
    }
}

static STORE: OnceLock<TaskStore> = OnceLock::new();

/// Install the process-wide task store (boot). Errors if already installed.
pub fn init(store: TaskStore) -> Result<(), TaskStore> {
    STORE.set(store)
}

/// The process-wide task store (an in-memory default when `init` was never called).
pub fn store_handle() -> &'static TaskStore {
    STORE.get_or_init(TaskStore::in_memory)
}

// ---- tools ----

/// The five Productivity tools for `agent_id`, scoped to the agent's world.
pub struct TasksTools {
    agent_id: String,
}

impl TasksTools {
    pub fn for_agent(agent_id: &str) -> Arc<dyn adk_core::Toolset> {
        Arc::new(Self { agent_id: agent_id.to_string() })
    }
}

#[async_trait::async_trait]
impl adk_core::Toolset for TasksTools {
    fn name(&self) -> &str {
        TOOLSET_ID
    }

    async fn tools(
        &self,
        _ctx: Arc<dyn adk_core::ReadonlyContext>,
    ) -> adk_core::Result<Vec<Arc<dyn adk_core::Tool>>> {
        Ok(vec![
            create_task_tool(&self.agent_id),
            list_tasks_tool(&self.agent_id),
            postpone_task_tool(&self.agent_id),
            complete_task_tool(&self.agent_id),
            plan_day_tool(&self.agent_id),
        ])
    }
}

fn scope_of(agent: &str) -> Domain {
    crate::tools::allowlist::catalog().world_for(agent)
}

fn arg_str<'a>(args: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty())
}

fn rejected(reason: impl Into<String>) -> adk_core::Result<serde_json::Value> {
    Ok(serde_json::json!({ "status": "rejected", "reason": reason.into() }))
}

fn task_json(task: &Task) -> serde_json::Value {
    serde_json::to_value(task).unwrap_or_default()
}

/// Fetch a task the agent may see, or explain why not.
async fn visible_task(user: &str, scope: Domain, args: &serde_json::Value) -> Result<Task, String> {
    let id = arg_str(args, "task_id")
        .and_then(|s| s.parse::<Uuid>().ok())
        .ok_or_else(|| "task_id (uuid from list_tasks) required".to_string())?;
    let task = store_handle().get(user, id).await.ok_or_else(|| format!("no task {id} for this user"))?;
    if !visible(scope, task.domain) {
        return Err(format!("task {id} belongs to the {} world, outside this agent's scope", task.domain));
    }
    Ok(task)
}

pub fn create_task_tool(agent_id: &str) -> Arc<dyn adk_core::Tool> {
    let agent = agent_id.to_string();
    Arc::new(FunctionTool::new(
        "create_task",
        "Create a task for the user. Arguments: {\"title\": \"…\" (required, ≤200 chars), \"kind\": \"task|deadline|focus_session|reminder|errand|household\" (default task), \"due\": RFC 3339 | \"YYYY-MM-DD\" | \"today\" | \"tomorrow\" (dates mean end of that day, UTC), \"duration_minutes\": 1–600 (focus sessions), \"priority\": \"low|normal|high\", \"notes\": \"…\" (≤500 chars), \"domain\": \"work|home|shared\" (default: your world)}. Runs immediately in suggest mode (write_local).",
        move |ctx, args| {
            let agent = agent.clone();
            async move {
                let scope = scope_of(&agent);
                let Some(title) = arg_str(&args, "title") else {
                    return rejected("title required");
                };
                if title.chars().count() > MAX_TITLE {
                    return rejected(format!("title must be ≤{MAX_TITLE} chars"));
                }
                let requested = arg_str(&args, "domain").and_then(Domain::parse);
                let domain = match write_domain(scope, requested) {
                    Ok(d) => d,
                    Err(reason) => return rejected(reason),
                };
                let kind = match arg_str(&args, "kind") {
                    None => TaskKind::default(),
                    Some(k) => match TaskKind::parse(k) {
                        Some(k) => k,
                        None => return rejected(format!("unknown kind '{k}'; expected task|deadline|focus_session|reminder|errand|household")),
                    },
                };
                let priority = match arg_str(&args, "priority") {
                    None => Priority::default(),
                    Some(p) => match Priority::parse(p) {
                        Some(p) => p,
                        None => return rejected(format!("unknown priority '{p}'; expected low|normal|high")),
                    },
                };
                let due = match parse_due(args.get("due"), Utc::now()) {
                    Ok(d) => d,
                    Err(reason) => return rejected(reason),
                };
                let duration_minutes = match args.get("duration_minutes").and_then(|v| v.as_i64()) {
                    None => None,
                    Some(m) if (1..=600).contains(&m) => Some(m as i32),
                    Some(_) => return rejected("duration_minutes must be between 1 and 600"),
                };
                let notes = arg_str(&args, "notes");
                if notes.is_some_and(|n| n.chars().count() > MAX_NOTES) {
                    return rejected(format!("notes must be ≤{MAX_NOTES} chars"));
                }
                let task = store_handle()
                    .create(
                        ctx.user_id(),
                        NewTask { domain, title, kind, due, duration_minutes, priority, source_agent: &agent, notes },
                    )
                    .await;
                Ok(serde_json::json!({
                    "status": "created",
                    "task": task_json(&task),
                    "message": "Task stored; it now appears in list_tasks and plan_day."
                }))
            }
        },
    ))
}

pub fn list_tasks_tool(agent_id: &str) -> Arc<dyn adk_core::Tool> {
    let agent = agent_id.to_string();
    Arc::new(
        FunctionTool::new(
            "list_tasks",
            "List the user's tasks within your world. Arguments (all optional): {\"status\": \"open|done|cancelled|all\" (default open), \"domain\": \"work|home|shared\", \"kind\": \"task|deadline|focus_session|reminder|errand|household\", \"due_before\": date, \"due_after\": date}. Due filters exclude undated tasks. Sorted due-soonest first, undated last.",
            move |ctx, args| {
                let agent = agent.clone();
                async move {
                    let scope = scope_of(&agent);
                    let status = match arg_str(&args, "status") {
                        None => Some(TaskStatus::Open),
                        Some("all") => None,
                        Some(s) => match TaskStatus::parse(s) {
                            Some(s) => Some(s),
                            None => return rejected(format!("unknown status '{s}'; expected open|done|cancelled|all")),
                        },
                    };
                    let domain = arg_str(&args, "domain").and_then(Domain::parse);
                    if let Some(d) = domain
                        && !visible(scope, d)
                    {
                        return rejected(format!("the {d} world is outside this agent's scope ({scope})"));
                    }
                    let kind = match arg_str(&args, "kind") {
                        None => None,
                        Some(k) => match TaskKind::parse(k) {
                            Some(k) => Some(k),
                            None => return rejected(format!("unknown kind '{k}'")),
                        },
                    };
                    let now = Utc::now();
                    let due_before = match parse_due(args.get("due_before"), now) {
                        Ok(d) => d,
                        Err(reason) => return rejected(reason),
                    };
                    let due_after = match parse_due(args.get("due_after"), now) {
                        Ok(d) => d.map(|d| d - Duration::seconds(86_399)),
                        Err(reason) => return rejected(reason),
                    };
                    let tasks = store_handle()
                        .list(ctx.user_id(), scope, &TaskFilter { domain, status, kind, due_before, due_after })
                        .await;
                    Ok(serde_json::json!({
                        "scope": scope.as_str(),
                        "count": tasks.len(),
                        "tasks": tasks.iter().map(task_json).collect::<Vec<_>>(),
                    }))
                }
            },
        )
        .with_read_only(true)
        .with_concurrency_safe(true),
    )
}

pub fn postpone_task_tool(agent_id: &str) -> Arc<dyn adk_core::Tool> {
    let agent = agent_id.to_string();
    Arc::new(FunctionTool::new(
        "postpone_task",
        "Move an open task's due date and count the postponement (the Balance Agent tracks repeated postponements). Arguments: {\"task_id\": uuid (required), \"due\": RFC 3339 | \"YYYY-MM-DD\" | \"today\" | \"tomorrow\" (default: one day later, or tomorrow when undated)}.",
        move |ctx, args| {
            let agent = agent.clone();
            async move {
                let scope = scope_of(&agent);
                let task = match visible_task(ctx.user_id(), scope, &args).await {
                    Ok(t) => t,
                    Err(reason) => return rejected(reason),
                };
                let due = match parse_due(args.get("due"), Utc::now()) {
                    Ok(d) => d,
                    Err(reason) => return rejected(reason),
                };
                match store_handle().postpone(ctx.user_id(), task.id, due, &agent).await {
                    Some(t) => Ok(serde_json::json!({
                        "status": "postponed",
                        "task": task_json(&t),
                        "message": format!("Postponed {} time(s) so far.", t.postponed_count)
                    })),
                    None => rejected(format!("task {} is not open", task.id)),
                }
            }
        },
    ))
}

pub fn complete_task_tool(agent_id: &str) -> Arc<dyn adk_core::Tool> {
    let agent = agent_id.to_string();
    Arc::new(FunctionTool::new(
        "complete_task",
        "Mark an open task as done. Arguments: {\"task_id\": uuid (required)}.",
        move |ctx, args| {
            let agent = agent.clone();
            async move {
                let scope = scope_of(&agent);
                let task = match visible_task(ctx.user_id(), scope, &args).await {
                    Ok(t) => t,
                    Err(reason) => return rejected(reason),
                };
                match store_handle().complete(ctx.user_id(), task.id, &agent).await {
                    Some(t) => Ok(serde_json::json!({ "status": "completed", "task": task_json(&t) })),
                    None => rejected(format!("task {} is not open", task.id)),
                }
            }
        },
    ))
}

pub fn plan_day_tool(agent_id: &str) -> Arc<dyn adk_core::Tool> {
    let agent = agent_id.to_string();
    Arc::new(
        FunctionTool::new(
            "plan_day",
            "Deterministic day plan from the user's open tasks within your world: overdue, due today, focus sessions, upcoming (next 3 days) and postponement debt (postponed ≥3 times). Arguments (optional): {\"date\": \"YYYY-MM-DD\" | \"today\" | \"tomorrow\" (default today, UTC), \"domain\": \"work|home|shared\"}. Present the facts; do not invent tasks.",
            move |ctx, args| {
                let agent = agent.clone();
                async move {
                    let scope = scope_of(&agent);
                    let now = Utc::now();
                    let day = match arg_str(&args, "date").map(str::to_ascii_lowercase).as_deref() {
                        None | Some("today") => now.date_naive(),
                        Some("tomorrow") => now.date_naive() + Duration::days(1),
                        Some(other) => match NaiveDate::parse_from_str(other, "%Y-%m-%d") {
                            Ok(d) => d,
                            Err(_) => return rejected(format!("could not parse date '{other}' — use YYYY-MM-DD, today or tomorrow")),
                        },
                    };
                    // The domain argument only narrows: the scope stays the agent's world, so a
                    // work agent asking for "shared" gets shared tasks, never the home world's.
                    let domain = arg_str(&args, "domain").and_then(Domain::parse);
                    if let Some(d) = domain
                        && !visible(scope, d)
                    {
                        return rejected(format!("the {d} world is outside this agent's scope ({scope})"));
                    }
                    let plan = store_handle().plan_day(ctx.user_id(), scope, domain, day).await;
                    Ok(serde_json::json!({ "status": "ok", "plan": plan }))
                }
            },
        )
        .with_read_only(true)
        .with_concurrency_safe(true),
    )
}

// ---- Postgres ----

#[derive(sqlx::FromRow)]
struct Row {
    id: Uuid,
    user_id: String,
    domain: String,
    title: String,
    kind: String,
    due: Option<DateTime<Utc>>,
    duration_minutes: Option<i32>,
    priority: String,
    status: String,
    postponed_count: i32,
    source_agent: String,
    notes: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
}

async fn load_pg(pool: &PgPool, user_id: &str) -> anyhow::Result<Vec<Task>> {
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, user_id, domain, title, kind, due, duration_minutes, priority, status, postponed_count, \
         source_agent, notes, created_at, updated_at, completed_at FROM tasks WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Task {
            id: r.id,
            user_id: r.user_id,
            domain: Domain::parse(&r.domain).unwrap_or_default(),
            title: r.title,
            kind: TaskKind::parse(&r.kind).unwrap_or_default(),
            due: r.due,
            duration_minutes: r.duration_minutes,
            priority: Priority::parse(&r.priority).unwrap_or_default(),
            status: TaskStatus::parse(&r.status).unwrap_or_default(),
            postponed_count: r.postponed_count,
            source_agent: r.source_agent,
            notes: r.notes,
            created_at: r.created_at,
            updated_at: r.updated_at,
            completed_at: r.completed_at,
        })
        .collect())
}

async fn upsert_pg(pool: &PgPool, t: &Task) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO tasks (id, user_id, domain, title, kind, due, duration_minutes, priority, status, postponed_count, \
         source_agent, notes, created_at, updated_at, completed_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15) \
         ON CONFLICT (id) DO UPDATE SET title = $4, kind = $5, due = $6, duration_minutes = $7, priority = $8, \
         status = $9, postponed_count = $10, notes = $12, updated_at = $14, completed_at = $15",
    )
    .bind(t.id)
    .bind(&t.user_id)
    .bind(t.domain.as_str())
    .bind(&t.title)
    .bind(t.kind.as_str())
    .bind(t.due)
    .bind(t.duration_minutes)
    .bind(t.priority.as_str())
    .bind(t.status.as_str())
    .bind(t.postponed_count)
    .bind(&t.source_agent)
    .bind(&t.notes)
    .bind(t.created_at)
    .bind(t.updated_at)
    .bind(t.completed_at)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new(domain: Domain, title: &str, due: Option<DateTime<Utc>>) -> NewTask<'_> {
        NewTask {
            domain,
            title,
            kind: TaskKind::Task,
            due,
            duration_minutes: None,
            priority: Priority::Normal,
            source_agent: "test",
            notes: None,
        }
    }

    #[test]
    fn due_parsing_accepts_rfc3339_dates_and_words() {
        let now = DateTime::parse_from_rfc3339("2026-09-20T10:00:00Z").unwrap().with_timezone(&Utc);
        assert_eq!(parse_due(None, now).unwrap(), None);
        assert_eq!(parse_due(Some(&serde_json::Value::Null), now).unwrap(), None);
        let exact = parse_due(Some(&serde_json::json!("2026-10-01T09:30:00+02:00")), now).unwrap().unwrap();
        assert_eq!(exact.to_rfc3339(), "2026-10-01T07:30:00+00:00");
        let date = parse_due(Some(&serde_json::json!("2026-10-01")), now).unwrap().unwrap();
        assert_eq!(date.to_rfc3339(), "2026-10-01T23:59:59+00:00");
        let tomorrow = parse_due(Some(&serde_json::json!("tomorrow")), now).unwrap().unwrap();
        assert_eq!(tomorrow.date_naive().to_string(), "2026-09-21");
        assert!(parse_due(Some(&serde_json::json!("next week")), now).is_err());
        assert!(parse_due(Some(&serde_json::json!(42)), now).is_err());
    }

    #[test]
    fn write_domain_keeps_world_agents_in_their_world() {
        assert_eq!(write_domain(Domain::Work, None), Ok(Domain::Work));
        assert_eq!(write_domain(Domain::Work, Some(Domain::Shared)), Ok(Domain::Shared));
        assert!(write_domain(Domain::Work, Some(Domain::Home)).is_err());
        assert_eq!(write_domain(Domain::Shared, None), Ok(Domain::Shared));
        assert_eq!(write_domain(Domain::Shared, Some(Domain::Home)), Ok(Domain::Home));
        assert!(visible(Domain::Home, Domain::Shared));
        assert!(!visible(Domain::Home, Domain::Work));
        assert!(visible(Domain::Shared, Domain::Work));
    }

    #[tokio::test]
    async fn list_sorts_due_first_then_priority_and_scopes_by_world() {
        let store = TaskStore::in_memory();
        let now = Utc::now();
        store.create("u", new(Domain::Work, "undated", None)).await;
        store
            .create(
                "u",
                NewTask { priority: Priority::High, ..new(Domain::Work, "soon-high", Some(now + Duration::hours(2))) },
            )
            .await;
        store.create("u", new(Domain::Work, "soon-normal", Some(now + Duration::hours(2)))).await;
        store.create("u", new(Domain::Home, "home-later", Some(now + Duration::days(2)))).await;
        store.create("u", new(Domain::Shared, "shared", None)).await;

        let work = store.list("u", Domain::Work, &TaskFilter::default()).await;
        let titles: Vec<&str> = work.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, vec!["soon-high", "soon-normal", "undated", "shared"]);
        let home = store.list("u", Domain::Home, &TaskFilter::default()).await;
        assert_eq!(home.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), vec!["home-later", "shared"]);
        assert_eq!(store.list("u", Domain::Shared, &TaskFilter::default()).await.len(), 5);
        let windowed = store
            .list("u", Domain::Shared, &TaskFilter { due_before: Some(now + Duration::hours(3)), ..Default::default() })
            .await;
        assert_eq!(windowed.len(), 2, "due filters exclude undated tasks");
    }

    #[tokio::test]
    async fn plan_day_groups_overdue_today_focus_upcoming_and_debt() {
        let store = TaskStore::in_memory();
        let day = NaiveDate::from_ymd_opt(2026, 9, 21).unwrap();
        let noon = start_of_day(day) + Duration::hours(12);
        store.create("u", new(Domain::Work, "overdue", Some(start_of_day(day) - Duration::days(1)))).await;
        store.create("u", new(Domain::Work, "today", Some(noon))).await;
        store
            .create("u", NewTask { kind: TaskKind::FocusSession, duration_minutes: Some(90), ..new(Domain::Work, "focus", Some(noon)) })
            .await;
        store.create("u", new(Domain::Work, "upcoming", Some(noon + Duration::days(2)))).await;
        store.create("u", new(Domain::Work, "far", Some(noon + Duration::days(30)))).await;
        let debt = store.create("u", new(Domain::Home, "passport", None)).await;
        for _ in 0..3 {
            store.postpone("u", debt.id, Some(noon + Duration::days(1)), "test").await.unwrap();
        }
        let done = store.create("u", new(Domain::Work, "done", Some(noon))).await;
        store.complete("u", done.id, "test").await.unwrap();
        assert!(store.complete("u", done.id, "test").await.is_none(), "completing twice is refused");

        let plan = store.plan_day("u", Domain::Shared, None, day).await;
        assert_eq!(plan.overdue.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), vec!["overdue"]);
        assert_eq!(plan.due_today.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), vec!["today"]);
        assert_eq!(plan.focus_sessions.len(), 1);
        assert_eq!(plan.upcoming.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), vec!["passport", "upcoming"]);
        assert_eq!(plan.postponed_debt.len(), 1);
        assert_eq!(plan.postponed_debt[0].postponed_count, 3);
        assert_eq!(plan.open_total, 6);
        let work_only = store.plan_day("u", Domain::Work, None, day).await;
        assert!(work_only.postponed_debt.is_empty(), "home debt is invisible to a work scope");
        // A domain only narrows the scope: asking a work scope for shared tasks never widens it.
        store.create("u", new(Domain::Shared, "shared-today", Some(noon))).await;
        let shared_from_work = store.plan_day("u", Domain::Work, Some(Domain::Shared), day).await;
        assert_eq!(shared_from_work.open_total, 1);
        assert_eq!(shared_from_work.due_today[0].title, "shared-today");
        assert!(shared_from_work.postponed_debt.is_empty());
        assert_eq!(store.plan_day("u", Domain::Work, Some(Domain::Home), day).await.open_total, 0);
    }
}
