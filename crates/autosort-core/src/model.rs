use std::path::PathBuf;

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------- conditions

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MatchMode {
    #[default]
    All,
    Any,
}

impl MatchMode {
    pub const ALL: [MatchMode; 2] = [MatchMode::All, MatchMode::Any];
    pub fn label(self) -> &'static str {
        match self {
            MatchMode::All => "all of the conditions",
            MatchMode::Any => "any of the conditions",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operator {
    Equals,
    NotEquals,
    Contains,
    DoesNotContain,
    StartsWith,
    EndsWith,
    MatchesRegex,
    GreaterThan,
    LessThan,
}

impl Operator {
    pub fn label(self) -> &'static str {
        match self {
            Operator::Equals => "is",
            Operator::NotEquals => "is not",
            Operator::Contains => "contains",
            Operator::DoesNotContain => "does not contain",
            Operator::StartsWith => "starts with",
            Operator::EndsWith => "ends with",
            Operator::MatchesRegex => "matches regex",
            Operator::GreaterThan => "is greater than",
            Operator::LessThan => "is less than",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Attribute {
    Name,
    Extension,
    Kind,
    SizeMb,
    DaysSinceModified,
    DaysSinceCreated,
    DaysSinceOpened,
    DaysSinceTaken,
    WhereFrom,
    Contents,
    Tag,
    Duplicate,
}

impl Attribute {
    pub const ALL: [Attribute; 12] = [
        Attribute::Name,
        Attribute::Extension,
        Attribute::Kind,
        Attribute::SizeMb,
        Attribute::DaysSinceModified,
        Attribute::DaysSinceCreated,
        Attribute::DaysSinceOpened,
        Attribute::DaysSinceTaken,
        Attribute::WhereFrom,
        Attribute::Contents,
        Attribute::Tag,
        Attribute::Duplicate,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Attribute::Name => "Name",
            Attribute::Extension => "Extension",
            Attribute::Kind => "Kind",
            Attribute::SizeMb => "Size (MB)",
            Attribute::DaysSinceModified => "Days since modified",
            Attribute::DaysSinceCreated => "Days since created",
            Attribute::DaysSinceOpened => "Days since last opened",
            Attribute::DaysSinceTaken => "Days since photo taken",
            Attribute::WhereFrom => "Downloaded from",
            Attribute::Contents => "Contents",
            Attribute::Tag => "Tag",
            Attribute::Duplicate => "Duplicate",
        }
    }

    pub fn is_numeric(self) -> bool {
        matches!(
            self,
            Attribute::SizeMb
                | Attribute::DaysSinceModified
                | Attribute::DaysSinceCreated
                | Attribute::DaysSinceOpened
                | Attribute::DaysSinceTaken
        )
    }

    /// True when the condition needs no value (the operator alone decides).
    pub fn is_valueless(self) -> bool {
        self == Attribute::Duplicate
    }

    /// Wording of an operator in the context of this attribute ("has tag" reads better than "is").
    pub fn operator_label(self, op: Operator) -> &'static str {
        match (self, op) {
            (Attribute::Tag, Operator::Equals) => "has tag",
            (Attribute::Tag, Operator::NotEquals) => "doesn't have tag",
            (Attribute::Tag, Operator::Contains) => "has a tag containing",
            (Attribute::Duplicate, Operator::Equals) => "is a duplicate",
            (Attribute::Duplicate, Operator::NotEquals) => "is not a duplicate",
            _ => op.label(),
        }
    }

    pub fn allowed_operators(self) -> &'static [Operator] {
        use Operator::*;
        match self {
            Attribute::Name => &[Contains, DoesNotContain, StartsWith, EndsWith, Equals, NotEquals, MatchesRegex],
            Attribute::Extension | Attribute::Kind => &[Equals, NotEquals],
            Attribute::WhereFrom => &[Contains, DoesNotContain, StartsWith, EndsWith, Equals, MatchesRegex],
            Attribute::Contents => &[Contains, DoesNotContain, MatchesRegex],
            Attribute::Tag => &[Equals, NotEquals, Contains],
            Attribute::Duplicate => &[Equals, NotEquals],
            _ => &[GreaterThan, LessThan],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Condition {
    #[serde(default = "Uuid::new_v4")]
    pub id: Uuid,
    pub attribute: Attribute,
    pub op: Operator,
    /// Text, a number, a `FileKind` key, or (for `Extension`) a comma separated list like "jpg, png".
    pub value: String,
}

impl Condition {
    pub fn new(attribute: Attribute, op: Operator, value: impl Into<String>) -> Self {
        Condition { id: Uuid::new_v4(), attribute, op, value: value.into() }
    }
}

impl Default for Condition {
    fn default() -> Self {
        Condition::new(Attribute::Extension, Operator::Equals, "")
    }
}

// ------------------------------------------------------------------- actions

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    Move,
    Copy,
    Rename,
    Trash,
    RunCommand,
    AddTags,
    OpenWith,
    RunShortcut,
}

impl ActionKind {
    pub const ALL: [ActionKind; 8] = [
        ActionKind::Move,
        ActionKind::Copy,
        ActionKind::Rename,
        ActionKind::AddTags,
        ActionKind::OpenWith,
        ActionKind::RunShortcut,
        ActionKind::RunCommand,
        ActionKind::Trash,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ActionKind::Move => "Move to folder",
            ActionKind::Copy => "Copy to folder",
            ActionKind::Rename => "Rename (keeps extension)",
            ActionKind::Trash => "Move to Trash / Recycle Bin",
            ActionKind::RunCommand => "Run shell command",
            ActionKind::AddTags => "Add tags",
            ActionKind::OpenWith => "Open (with app)",
            ActionKind::RunShortcut => "Run Shortcut (macOS)",
        }
    }

    pub fn placeholder(self) -> &'static str {
        match self {
            ActionKind::Move | ActionKind::Copy => "~/Pictures/{kind}/{year}",
            ActionKind::Rename => "{date} {name}",
            ActionKind::Trash => "",
            ActionKind::RunCommand => "cp \"$AUTOSORT_FILE\" ~/Backups/",
            ActionKind::AddTags => "Red, Work, Invoice",
            ActionKind::OpenWith => "app name (blank = default app)",
            ActionKind::RunShortcut => "name of the Shortcut",
        }
    }

    pub fn uses_parameter(self) -> bool {
        self != ActionKind::Trash
    }

    /// Whether an empty value is acceptable.
    pub fn parameter_optional(self) -> bool {
        self == ActionKind::OpenWith
    }

    pub fn uses_conflict_policy(self) -> bool {
        matches!(self, ActionKind::Move | ActionKind::Copy | ActionKind::Rename)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConflictPolicy {
    /// Keep both: "report.pdf" -> "report 2.pdf".
    #[default]
    Rename,
    Skip,
    Overwrite,
}

impl ConflictPolicy {
    pub const ALL: [ConflictPolicy; 3] = [ConflictPolicy::Rename, ConflictPolicy::Skip, ConflictPolicy::Overwrite];
    pub fn label(self) -> &'static str {
        match self {
            ConflictPolicy::Rename => "keep both (add a number)",
            ConflictPolicy::Skip => "skip",
            ConflictPolicy::Overwrite => "replace",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Action {
    #[serde(default = "Uuid::new_v4")]
    pub id: Uuid,
    pub kind: ActionKind,
    #[serde(default)]
    pub parameter: String,
    #[serde(default)]
    pub conflict: ConflictPolicy,
}

impl Action {
    pub fn new(kind: ActionKind, parameter: impl Into<String>) -> Self {
        Action { id: Uuid::new_v4(), kind, parameter: parameter.into(), conflict: ConflictPolicy::Rename }
    }
}

// ----------------------------------------------------------- rules & folders

fn yes() -> bool {
    true
}

/// When a rule is allowed to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Schedule {
    /// Continuously, as files arrive.
    #[default]
    Always,
    Daily { hour: u8, minute: u8 },
    /// `weekday`: 0 = Monday … 6 = Sunday.
    Weekly { weekday: u8, hour: u8, minute: u8 },
}

pub const WEEKDAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];

impl Schedule {
    pub fn summary(&self) -> String {
        match *self {
            Schedule::Always => "always".to_string(),
            Schedule::Daily { hour, minute } => format!("daily at {hour:02}:{minute:02}"),
            Schedule::Weekly { weekday, hour, minute } => {
                format!("every {} at {hour:02}:{minute:02}", WEEKDAYS[(weekday % 7) as usize])
            }
        }
    }

    /// The most recent moment (not after `now`) this schedule fired. `None` for `Always`.
    pub fn latest_occurrence(&self, now: DateTime<Local>) -> Option<DateTime<Local>> {
        use chrono::{Datelike, Duration, TimeZone};
        let at = |date: chrono::NaiveDate, hour: u8, minute: u8| {
            Local.from_local_datetime(&date.and_hms_opt(hour.min(23) as u32, minute.min(59) as u32, 0)?).earliest()
        };
        match *self {
            Schedule::Always => None,
            Schedule::Daily { hour, minute } => {
                let today = now.date_naive();
                let cand = at(today, hour, minute)?;
                if cand <= now { Some(cand) } else { at(today - Duration::days(1), hour, minute) }
            }
            Schedule::Weekly { weekday, hour, minute } => (0..=7).find_map(|back| {
                let date = now.date_naive() - Duration::days(back);
                if date.weekday().num_days_from_monday() as u8 != weekday % 7 {
                    return None;
                }
                at(date, hour, minute).filter(|c| *c <= now)
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    #[serde(default = "Uuid::new_v4")]
    pub id: Uuid,
    pub name: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub match_mode: MatchMode,
    #[serde(default)]
    pub conditions: Vec<Condition>,
    #[serde(default)]
    pub actions: Vec<Action>,
    /// When true (default) no later rule is evaluated for a file once this one matched.
    #[serde(default = "yes")]
    pub stop_processing: bool,
    /// Learning mode: a rule that is not approved only shows up in Preview and never changes files.
    #[serde(default = "yes")]
    pub approved: bool,
    #[serde(default)]
    pub schedule: Schedule,
}

impl Rule {
    pub fn new(name: impl Into<String>) -> Self {
        Rule {
            id: Uuid::new_v4(),
            name: name.into(),
            enabled: true,
            match_mode: MatchMode::All,
            conditions: vec![],
            actions: vec![],
            stop_processing: true,
            approved: true,
            schedule: Schedule::Always,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WatchedFolder {
    #[serde(default = "Uuid::new_v4")]
    pub id: Uuid,
    pub path: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub include_subfolders: bool,
    #[serde(default)]
    pub rules: Vec<Rule>,
    /// Glob patterns (one per entry) for names or paths rules must never touch, e.g. `*.psd`, `Archive/*`.
    #[serde(default)]
    pub ignore: Vec<String>,
}

impl WatchedFolder {
    pub fn new(path: impl Into<String>) -> Self {
        WatchedFolder {
            id: Uuid::new_v4(),
            path: path.into(),
            enabled: true,
            include_subfolders: false,
            rules: vec![],
            ignore: vec![],
        }
    }

    pub fn root(&self) -> PathBuf {
        crate::template::expand_tilde(&self.path)
    }

    pub fn display_name(&self) -> String {
        self.root().file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| self.path.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub folders: Vec<WatchedFolder>,
    /// Files modified more recently than this are left alone (they may still be downloading or copying).
    #[serde(default = "default_settle")]
    pub settle_seconds: u64,
    /// Show a desktop notification when AutoSort files something (or fails to).
    #[serde(default = "yes")]
    pub notifications: bool,
    /// New rules start unapproved (preview-only) until you approve them.
    #[serde(default = "yes")]
    pub learning_mode: bool,
}

fn default_settle() -> u64 {
    5
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig { folders: vec![], settle_seconds: default_settle(), notifications: true, learning_mode: true }
    }
}

// -------------------------------------------------------------- activity log

/// How to reverse an action that changed the disk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UndoOp {
    /// A move or rename: put the file back at `from`.
    Moved { from: PathBuf, to: PathBuf },
    /// A copy: send the copy to the Trash.
    Copied { copy: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityEntry {
    #[serde(default = "Uuid::new_v4")]
    pub id: Uuid,
    #[serde(default)]
    pub undo: Option<UndoOp>,
    #[serde(default)]
    pub undone: bool,
    pub time: DateTime<Local>,
    pub folder: String,
    pub rule: String,
    pub file: String,
    pub action: String,
    pub detail: String,
    pub success: bool,
    pub dry_run: bool,
}

// ------------------------------------------------------------- plain-English summaries

impl Condition {
    /// e.g. "extension is jpg, png" or "kind is Images".
    pub fn summary(&self) -> String {
        if self.attribute.is_valueless() {
            return format!("file {}", self.attribute.operator_label(self.op));
        }
        let value = match self.attribute {
            Attribute::Kind => crate::kind::FileKind::from_key(&self.value)
                .map(|k| k.display_name().to_string())
                .unwrap_or_else(|| self.value.trim().to_string()),
            _ => self.value.trim().to_string(),
        };
        let value = if value.is_empty() { "…".to_string() } else { value };
        match self.attribute {
            Attribute::Tag => format!("{} {}", self.attribute.operator_label(self.op), value),
            _ => format!("{} {} {}", self.attribute.label().to_lowercase(), self.op.label(), value),
        }
    }
}

impl Action {
    /// e.g. "move to {folder}/Sorted/{kind}" or "move to Trash".
    pub fn summary(&self) -> String {
        let p = self.parameter.trim();
        let p = if p.is_empty() { "…" } else { p };
        match self.kind {
            ActionKind::Move => format!("move to {p}"),
            ActionKind::Copy => format!("copy to {p}"),
            ActionKind::Rename => format!("rename to {p}"),
            ActionKind::Trash => "move to Trash".to_string(),
            ActionKind::RunCommand => "run a command".to_string(),
            ActionKind::AddTags => format!("add tags {p}"),
            ActionKind::OpenWith if self.parameter.trim().is_empty() => "open".to_string(),
            ActionKind::OpenWith => format!("open with {p}"),
            ActionKind::RunShortcut => format!("run Shortcut {p}"),
        }
    }
}

impl Rule {
    /// One line describing the rule, e.g. "If extension is pdf, then move to ~/Documents".
    pub fn summary(&self) -> String {
        if self.conditions.is_empty() {
            return "No conditions yet — this rule never matches".to_string();
        }
        let joiner = if self.match_mode == MatchMode::All { " and " } else { " or " };
        let conds = self.conditions.iter().map(Condition::summary).collect::<Vec<_>>().join(joiner);
        if self.actions.is_empty() {
            return format!("If {conds}, then nothing yet");
        }
        let acts = self.actions.iter().map(Action::summary).collect::<Vec<_>>().join(", then ");
        format!("If {conds}, then {acts}")
    }

    /// Short badges for the rule list, e.g. ["daily at 09:00"].
    pub fn tags(&self) -> Vec<String> {
        let mut t = Vec::new();
        if self.schedule != Schedule::Always {
            t.push(self.schedule.summary());
        }
        t
    }
}
