use crate::model::*;

/// Sensible starter rules for a Downloads folder.
pub fn downloads_starter_rules() -> Vec<Rule> {
    let mut installers = Rule::new("Trash old installers");
    installers.conditions = vec![
        Condition::new(Attribute::Kind, Operator::Equals, "installer"),
        Condition::new(Attribute::DaysSinceModified, Operator::GreaterThan, "14"),
    ];
    installers.actions = vec![Action::new(ActionKind::Trash, "")];

    let mut screenshots = Rule::new("File screenshots");
    screenshots.conditions = vec![
        Condition::new(Attribute::Name, Operator::StartsWith, "Screenshot"),
        Condition::new(Attribute::Kind, Operator::Equals, "image"),
    ];
    screenshots.actions = vec![Action::new(ActionKind::Move, "{folder}/Sorted/Screenshots/{year}-{month}")];

    let mut by_kind = Rule::new("Sort by kind");
    by_kind.conditions = vec![Condition::new(Attribute::Kind, Operator::NotEquals, "other")];
    by_kind.actions = vec![Action::new(ActionKind::Move, "{folder}/Sorted/{kind}")];

    vec![installers, screenshots, by_kind]
}
