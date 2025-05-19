use serde::Serialize;
use std::fmt::Display;
use std::path::PathBuf;
use std::str::FromStr;
use winnow::Result;
use winnow::ascii::{newline, space0, space1};
use winnow::combinator::{alt, separated};
use winnow::combinator::{opt, seq};
use winnow::error::ContextError;
use winnow::prelude::*;
use winnow::token::{rest, take_till, take_until, take_while};
use winnow_parse_error::ParseError;

const EMPTY_DESCRIPTION: &str = "(no description set)";

#[derive(Debug, PartialEq, Eq, Serialize)]
enum FileStatus {
    Added,
    Modified,
    Removed,
}

impl Display for FileStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let symbol = match self {
            FileStatus::Added => 'A',
            FileStatus::Modified => 'M',
            FileStatus::Removed => 'R',
        };
        write!(f, "{symbol}")
    }
}

fn file_status(s: &mut &str) -> Result<FileStatus> {
    alt((
        'A'.map(|_| FileStatus::Added),
        'R'.map(|_| FileStatus::Removed),
        'M'.map(|_| FileStatus::Modified),
    ))
    .parse_next(s)
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct WorkingCopyChange {
    status: FileStatus,
    path: PathBuf,
}

impl Display for WorkingCopyChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.status, self.path.display())
    }
}

fn part<'a>(s: &mut &'a str) -> Result<&'a str> {
    take_till(1.., |c: char| c == '/' || c == '\n').parse_next(s)
}

fn path(s: &mut &str) -> Result<PathBuf> {
    let parts: Vec<&str> = separated(1.., part, "/").parse_next(s)?;
    let path: PathBuf = parts.iter().collect();
    Ok(path)
}

fn file_change(s: &mut &str) -> Result<WorkingCopyChange> {
    seq! {WorkingCopyChange {
        status: file_status,
        _: space1,
        path: path
    }}
    .parse_next(s)
}

fn file_no_changes(s: &mut &str) -> Result<Vec<WorkingCopyChange>> {
    let _ = "The working copy has no changes.".parse_next(s)?;
    Ok(Vec::new())
}

fn file_yes_changes(s: &mut &str) -> Result<Vec<WorkingCopyChange>> {
    let _ = opt("Working copy changes:\n").parse_next(s)?; // TODO: I actually don't think this should be optional
    separated(0.., file_change, "\n").parse_next(s)
}

fn file_changes(s: &mut &str) -> Result<Vec<WorkingCopyChange>> {
    alt((file_no_changes, file_yes_changes)).parse_next(s)
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct CommitDetails {
    change_id: String,
    commit_id: String,
    empty: bool,
    bookmark: Option<String>,
    description: Option<String>,
}

impl CommitDetails {
    pub fn change_id(&self) -> &str {
        &self.change_id.as_str()
    }

    pub fn commit_id(&self) -> &str {
        &self.commit_id.as_str()
    }

    pub fn empty(&self) -> bool {
        self.empty
    }

    pub fn bookmark(&self) -> Option<&String> {
        self.bookmark.as_ref()
    }

    pub fn description(&self) -> &str {
        match &self.description {
            Some(description) => description.as_str(),
            None => EMPTY_DESCRIPTION,
        }
    }
}

impl Display for CommitDetails {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let empty = if self.empty { "(empty)" } else { "" };
        let bookmark = match &self.bookmark {
            Some(bookmark) => {
                format!("{bookmark} | ")
            }
            None => String::new(),
        };
        let description = match &self.description {
            Some(description) => &description,
            None => EMPTY_DESCRIPTION,
        };
        write!(
            f,
            "{} {} {empty}{bookmark}{description}",
            self.change_id, self.commit_id
        )
    }
}

fn char_between_inclusive(c: char, lower: char, upper: char) -> bool {
    c >= lower && c <= upper
}

fn change_id(s: &mut &str) -> Result<String> {
    take_while(1.., |c: char| char_between_inclusive(c, 'k', 'z'))
        .map(|s: &str| s.to_string())
        .parse_next(s)
}

fn commit_id(s: &mut &str) -> Result<String> {
    take_while(1.., |c: char| {
        char_between_inclusive(c, '0', '9') || char_between_inclusive(c, 'a', 'f')
    })
    .map(|s: &str| s.to_string())
    .parse_next(s)
}

use winnow::combinator::peek;
fn bookmark(s: &mut &str) -> Result<String> {
    let bookmark = peek(take_until(1.., " |").map(|x: &str| x.to_string())).parse_next(s)?;
    if bookmark.contains("\n") {
        // Without this peek check, the bookmark would capture all the way to the next line's bookmark
        return Err(ContextError::new());
    }
    let bookmark = take_until(1.., " |")
        .map(|x: &str| x.to_string())
        .parse_next(s)?;

    let _ = " |".parse_next(s)?;
    Ok(bookmark)
}

fn description(s: &mut &str) -> Result<Option<String>> {
    alt((
        "(no description set)".map(|_| None),
        alt((take_till(1.., |c: char| c == '\n'), rest)).map(|s: &str| Some(s.to_string())),
    ))
    .parse_next(s)
}

fn empty(s: &mut &str) -> Result<bool> {
    opt("(empty) ")
        .map(|x| match x {
            Some(_) => true,
            None => false,
        })
        .parse_next(s)
}

fn commit_details(s: &mut &str) -> Result<CommitDetails> {
    seq! {CommitDetails {
        change_id: change_id,
        _: space1,
        commit_id: commit_id,
        _: space1,
        empty: empty,
        _: space0,
        bookmark: opt(bookmark),
        _: space0,
        description: description,
    }}
    .parse_next(s)
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Status {
    file_changes: Vec<WorkingCopyChange>,
    working_copy: Commit,
    parent_commit: Commit,
}

impl Status {
    pub fn file_changes(&self) -> &[WorkingCopyChange] {
        &self.file_changes.as_ref()
    }

    pub fn working_copy(&self) -> &Commit {
        &self.working_copy
    }

    pub fn parent_commit(&self) -> &Commit {
        &self.parent_commit
    }
}

fn working_copy(s: &mut &str) -> Result<Commit> {
    let _ = "Working copy".parse_next(s)?;
    let _ = space1.parse_next(s)?;
    let _ = ":".parse_next(s)?;
    let _ = space1.parse_next(s)?;
    commit_details
        .map(|details| Commit::WorkingCopy(details))
        .parse_next(s)
}

fn parent_commit(s: &mut &str) -> Result<Commit> {
    let _ = "Parent commit:".parse_next(s)?;
    let _ = space1.parse_next(s)?;
    commit_details
        .map(|details| Commit::ParentCommit(details))
        .parse_next(s)
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "change_type")]
pub enum Commit {
    WorkingCopy(CommitDetails),
    ParentCommit(CommitDetails),
}

impl Commit {
    pub fn change_id(&self) -> &str {
        match self {
            Self::WorkingCopy(details) | Self::ParentCommit(details) => details.change_id(),
        }
    }

    pub fn commit_id(&self) -> &str {
        match self {
            Self::WorkingCopy(details) | Self::ParentCommit(details) => details.commit_id(),
        }
    }

    pub fn empty(&self) -> bool {
        match self {
            Self::WorkingCopy(details) | Self::ParentCommit(details) => details.empty(),
        }
    }

    pub fn bookmark(&self) -> Option<&String> {
        match self {
            Self::WorkingCopy(details) | Self::ParentCommit(details) => details.bookmark(),
        }
    }

    pub fn description(&self) -> &str {
        match self {
            Self::WorkingCopy(details) | Self::ParentCommit(details) => details.description(),
        }
    }
}

impl Display for Commit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WorkingCopy(details) => {
                write!(f, "{details}")
            }
            Self::ParentCommit(details) => {
                write!(f, "{details}")
            }
        }
    }
}

fn status(s: &mut &str) -> Result<Status> {
    seq! {Status {
        file_changes: file_changes,
        _: opt(newline),
        working_copy: working_copy,
        _: newline,
        parent_commit: parent_commit,
    }}
    .parse_next(s)
}

impl FromStr for Status {
    type Err = ParseError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        status.parse(s).map_err(|e| ParseError::from_parse(e))
    }
}

impl Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for change in &self.file_changes {
            write!(f, "{change}")?;
        }
        write!(f, "Working copy : {}", self.working_copy)?;
        write!(f, "Parent commit: {}", self.parent_commit)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use s_string::s;
    use winnow::error::ContextError;

    const HEADER: &str = "Working copy changes:";
    const FILE1: &str = "A src/lib.rs";
    const FILE2: &str = "A src/main.rs";
    const WORKING: &str = "Working copy : qnxonnkx 60be3879 main | (no description set)";
    const PARENT: &str = "Parent commit: zzzzzzzz 00000000 (empty) (no description set)";

    #[test]
    fn test_parse_change_id() {
        let mut input = "qnxonnkx";
        let expected = String::from("qnxonnkx");
        let actual = change_id(&mut input);
        assert_eq!(Ok(expected), actual);
        assert_eq!("", input);
    }

    #[test]
    fn test_parse_commit_id() {
        let mut input = "60be3879";
        let expected = String::from("60be3879");
        let actual = commit_id(&mut input);
        assert_eq!(Ok(expected), actual);
        assert_eq!("", input);
    }

    #[test]
    fn test_parse_file_change() {
        let mut input = FILE1;
        let expected = WorkingCopyChange {
            status: FileStatus::Added,
            path: PathBuf::from("src/lib.rs"),
        };
        let actual = file_change(&mut input);
        assert_eq!(Ok(expected), actual);
        assert_eq!("", input);
    }

    #[test]
    fn test_parse_file_changes() {
        let input = [HEADER, FILE1, FILE2].join("\n");
        let mut input = input.as_str();

        let expected = vec![
            WorkingCopyChange {
                status: FileStatus::Added,
                path: PathBuf::from("src/lib.rs"),
            },
            WorkingCopyChange {
                status: FileStatus::Added,
                path: PathBuf::from("src/main.rs"),
            },
        ];
        let actual = file_changes(&mut input);
        assert_eq!(Ok(expected), actual);
        assert_eq!("", input);
    }

    #[test]
    fn test_parse_details_1() {
        let mut input = "qnxonnkx 60be3879 main | (no description set)";
        let expected = CommitDetails {
            change_id: String::from("qnxonnkx"),
            commit_id: String::from("60be3879"),
            empty: false,
            bookmark: Some(String::from("main")),
            description: None,
        };
        let actual = commit_details(&mut input);
        assert_eq!(Ok(expected), actual)
    }

    #[test]
    fn test_parse_details_2() {
        let mut input = "zzzzzzzz 00000000 (empty) (no description set)";
        let expected = CommitDetails {
            change_id: s!("zzzzzzzz"),
            commit_id: s!("00000000"),
            empty: true,
            bookmark: None,
            description: None,
        };
        let actual = commit_details(&mut input);
        assert_eq!(Ok(expected), actual)
    }

    #[test]
    fn test_parse_working_copy() {
        let mut input = WORKING;
        let expected = Commit::WorkingCopy(CommitDetails {
            change_id: s!("qnxonnkx"),
            commit_id: s!("60be3879"),
            empty: false,
            bookmark: Some(String::from("main")),
            description: None,
        });
        let actual = working_copy(&mut input);
        assert_eq!(Ok(expected), actual);
        assert_eq!("", input);
    }

    #[test]
    fn test_parse_empty_description() {
        let mut input = "(no description set)";
        let expected = None;
        let actual = description(&mut input);
        assert_eq!(Ok(expected), actual);
        assert_eq!("", input);
    }

    #[test]
    fn test_parse_parent_commit() {
        let mut input = PARENT;
        let expected = Commit::ParentCommit(CommitDetails {
            change_id: s!("zzzzzzzz"),
            commit_id: s!("00000000"),
            empty: true,
            bookmark: None,
            description: None,
        });
        let actual = parent_commit(&mut input);
        assert_eq!(Ok(expected), actual);
        assert_eq!("", input);
    }

    #[test]
    fn test_status_from_str() {
        let input = [HEADER, FILE1, FILE2, WORKING, PARENT].join("\n");

        let expected = Status {
            file_changes: vec![
                WorkingCopyChange {
                    status: FileStatus::Added,
                    path: PathBuf::from("src/lib.rs"),
                },
                WorkingCopyChange {
                    status: FileStatus::Added,
                    path: PathBuf::from("src/main.rs"),
                },
            ],
            working_copy: Commit::WorkingCopy(CommitDetails {
                change_id: s!("qnxonnkx"),
                commit_id: s!("60be3879"),
                empty: false,
                bookmark: Some(s!("main")),
                description: None,
            }),
            parent_commit: Commit::ParentCommit(CommitDetails {
                change_id: s!("zzzzzzzz"),
                commit_id: s!("00000000"),
                empty: true,
                bookmark: None,
                description: None,
            }),
        };
        let actual = Status::from_str(&input);
        assert_eq!(Ok(expected), actual);
    }

    #[test]
    fn test_no_changes_status_from_str() {
        let input = ["The working copy has no changes.", WORKING, PARENT].join("\n");

        let expected = Status {
            file_changes: Vec::new(),
            working_copy: Commit::WorkingCopy(CommitDetails {
                change_id: s!("qnxonnkx"),
                commit_id: s!("60be3879"),
                empty: false,
                bookmark: Some(s!("main")),
                description: None,
            }),
            parent_commit: Commit::ParentCommit(CommitDetails {
                change_id: s!("zzzzzzzz"),
                commit_id: s!("00000000"),
                empty: true,
                bookmark: None,
                description: None,
            }),
        };
        let actual = Status::from_str(&input);
        assert_eq!(Ok(expected), actual);
    }

    #[test]
    fn test_parse_working_copy_2() {
        let mut input = "Working copy : oonwmqxn a3d80cec (no description set)";
        let expected = Commit::WorkingCopy(CommitDetails {
            change_id: s!("oonwmqxn"),
            commit_id: s!("a3d80cec"),
            empty: false,
            bookmark: None,
            description: None,
        });
        let actual = working_copy(&mut input);
        assert_eq!(Ok(expected), actual);
        assert_eq!("", input);
    }

    #[test]
    fn test_parse_parent_commit_2() {
        let mut input = "Parent commit: xtryyrqp 75d612e0 main@origin | main branch";
        let expected = Commit::ParentCommit(CommitDetails {
            change_id: s!("xtryyrqp"),
            commit_id: s!("75d612e0"),
            empty: false,
            bookmark: Some(s!("main@origin")),
            description: Some(s!("main branch")),
        });
        let actual = parent_commit(&mut input);
        assert_eq!(Ok(expected), actual);
        assert_eq!("", input);
    }

    #[test]
    fn test_status_from_str_2() {
        let mut input = r#"Working copy changes:
M src/lib.rs
Working copy : oonwmqxn a3d80cec (no description set)
Parent commit: xtryyrqp 75d612e0 main@origin | main branch"#;

        let _ = file_changes.parse_next(&mut input).unwrap();
        assert_eq!(
            r#"
Working copy : oonwmqxn a3d80cec (no description set)
Parent commit: xtryyrqp 75d612e0 main@origin | main branch"#,
            input
        );

        let _ = newline::<&str, ContextError>
            .parse_next(&mut input)
            .unwrap();
        assert_eq!(
            r#"Working copy : oonwmqxn a3d80cec (no description set)
Parent commit: xtryyrqp 75d612e0 main@origin | main branch"#,
            input
        );

        let foo = working_copy.parse_next(&mut input).unwrap();
        assert_eq!(
            Commit::WorkingCopy(CommitDetails {
                change_id: s!("oonwmqxn"),
                commit_id: s!("a3d80cec"),
                empty: false,
                bookmark: None,
                description: None
            }),
            foo
        );
        assert_eq!(
            r#"
Parent commit: xtryyrqp 75d612e0 main@origin | main branch"#,
            input
        );

        let _ = newline::<&str, ContextError>
            .parse_next(&mut input)
            .unwrap();
        assert_eq!(
            "Parent commit: xtryyrqp 75d612e0 main@origin | main branch",
            input
        );

        let _ = parent_commit.parse_next(&mut input).unwrap();
        assert_eq!("", input);

        let input = r#"Working copy changes:
M src/lib.rs
Working copy : oonwmqxn a3d80cec (no description set)
Parent commit: xtryyrqp 75d612e0 main@origin | main branch"#;

        // TODO: bookmark should be a struct with branch name and Option<Remote>

        let expected = Status {
            file_changes: vec![WorkingCopyChange {
                status: FileStatus::Modified,
                path: PathBuf::from("src/lib.rs"),
            }],
            working_copy: Commit::WorkingCopy(CommitDetails {
                change_id: s!("oonwmqxn"),
                commit_id: s!("a3d80cec"),
                empty: false,
                bookmark: None,
                description: None,
            }),
            parent_commit: Commit::ParentCommit(CommitDetails {
                change_id: s!("xtryyrqp"),
                commit_id: s!("75d612e0"),
                empty: false,
                bookmark: Some(s!("main@origin")),
                description: Some(s!("main branch")),
            }),
        };
        let actual = Status::from_str(&input);
        assert_eq!(Ok(expected), actual);
    }

    #[test]
    fn test_status_no_changes() {
        let input = [WORKING, PARENT].join("\n");
        let expected = Status {
            file_changes: Vec::new(),
            working_copy: Commit::WorkingCopy(CommitDetails {
                change_id: s!("qnxonnkx"),
                commit_id: s!("60be3879"),
                empty: false,
                bookmark: Some(s!("main")),
                description: None,
            }),
            parent_commit: Commit::ParentCommit(CommitDetails {
                change_id: s!("zzzzzzzz"),
                commit_id: s!("00000000"),
                empty: true,
                bookmark: None,
                description: None,
            }),
        };
        let actual = Status::from_str(&input);
        assert_eq!(Ok(expected), actual);
    }
}
