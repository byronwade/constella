use similar::{ChangeTag, TextDiff};
use std::path::PathBuf;

pub struct ContentDiff {
    pub path: PathBuf,
    pub changes: Vec<DiffChange>,
    pub change_percentage: f32,
    pub is_significant: bool,
}

#[derive(Debug)]
pub struct DiffChange {
    pub operation: ChangeOperation,
    pub content: String,
    pub line_number: usize,
}

#[derive(Debug)]
pub enum ChangeOperation {
    Added,
    Removed,
    Modified,
}

impl ContentDiff {
    pub fn new(old_content: &str, new_content: &str, path: PathBuf) -> Self {
        let diff = TextDiff::from_lines(old_content, new_content);
        let mut changes = Vec::new();
        let mut changed_lines = 0;
        let total_lines = old_content.lines().count().max(new_content.lines().count());

        for (idx, change) in diff.iter_all_changes().enumerate() {
            match change.tag() {
                ChangeTag::Delete => {
                    changes.push(DiffChange {
                        operation: ChangeOperation::Removed,
                        content: change.to_string(),
                        line_number: idx,
                    });
                    changed_lines += 1;
                }
                ChangeTag::Insert => {
                    changes.push(DiffChange {
                        operation: ChangeOperation::Added,
                        content: change.to_string(),
                        line_number: idx,
                    });
                    changed_lines += 1;
                }
                ChangeTag::Equal => {}
            }
        }

        let change_percentage = (changed_lines as f32 / total_lines as f32) * 100.0;
        let is_significant = change_percentage > 5.0; // Consider changes significant if > 5% changed

        Self {
            path,
            changes,
            change_percentage,
            is_significant,
        }
    }
} 