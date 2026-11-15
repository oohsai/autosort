use chrono::{DateTime, Local};
use regex::RegexBuilder;

use crate::fileinfo::FileInfo;
use crate::kind::FileKind;
use crate::model::{Attribute, Condition, MatchMode, Operator, Rule};

impl Condition {
    pub fn matches(&self, f: &FileInfo, now: DateTime<Local>) -> bool {
        let days = |t: DateTime<Local>| (now - t).num_milliseconds() as f64 / 86_400_000.0;
        match self.attribute {
            Attribute::Name => compare_text(&f.name, self.op, &self.value),
            Attribute::Extension => {
                let wanted: Vec<String> =
                    self.value.split(',').map(normalize_ext).filter(|e| !e.is_empty()).collect();
                match self.op {
                    Operator::Equals => wanted.contains(&f.ext),
                    Operator::NotEquals => !wanted.contains(&f.ext),
                    op => compare_text(&f.ext, op, &normalize_ext(&self.value)),
                }
            }
            Attribute::Kind => {
                let wanted = FileKind::from_key(&self.value);
                match self.op {
                    Operator::Equals => wanted == Some(f.kind()),
                    Operator::NotEquals => wanted != Some(f.kind()),
                    _ => false,
                }
            }
            Attribute::SizeMb => compare_number(f.size as f64 / 1_048_576.0, self.op, &self.value),
            Attribute::DaysSinceModified => compare_number(days(f.modified), self.op, &self.value),
            Attribute::DaysSinceCreated => compare_number(days(f.created), self.op, &self.value),
        }
    }
}

impl Rule {
    pub fn matches(&self, f: &FileInfo, now: DateTime<Local>) -> bool {
        if self.conditions.is_empty() {
            return false; // an empty rule must never match everything
        }
        match self.match_mode {
            MatchMode::All => self.conditions.iter().all(|c| c.matches(f, now)),
            MatchMode::Any => self.conditions.iter().any(|c| c.matches(f, now)),
        }
    }
}

fn normalize_ext(s: &str) -> String {
    s.trim().trim_start_matches('.').to_lowercase()
}

fn compare_text(lhs: &str, op: Operator, rhs: &str) -> bool {
    let a = lhs.to_lowercase();
    let b = rhs.trim().to_lowercase();
    match op {
        Operator::Equals => a == b,
        Operator::NotEquals => a != b,
        Operator::Contains => !b.is_empty() && a.contains(&b),
        Operator::DoesNotContain => b.is_empty() || !a.contains(&b),
        Operator::StartsWith => !b.is_empty() && a.starts_with(&b),
        Operator::EndsWith => !b.is_empty() && a.ends_with(&b),
        Operator::MatchesRegex => RegexBuilder::new(rhs).case_insensitive(true).build().map(|re| re.is_match(lhs)).unwrap_or(false),
        Operator::GreaterThan | Operator::LessThan => false,
    }
}

fn compare_number(lhs: f64, op: Operator, rhs: &str) -> bool {
    let Ok(n) = rhs.trim().parse::<f64>() else { return false };
    match op {
        Operator::GreaterThan => lhs > n,
        Operator::LessThan => lhs < n,
        Operator::Equals => (lhs - n).abs() < 1e-4,
        _ => false,
    }
}
