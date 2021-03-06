use chrono::prelude::*;
use chrono::Duration;
use crossterm::style::{style, Attribute, Color};
use crossterm::terminal;
use git2::{BranchType, Oid, Repository};
use std::convert::TryFrom;
use std::io;
use std::io::{Bytes, Read, Stdin, Stdout, Write};
use std::string::FromUtf8Error;

type Result<T, E = Error> = std::result::Result<T, E>;

fn main() {
    let result = (|| -> Result<_> {
        let repo = Repository::open_from_env()?;
        terminal::enable_raw_mode()?;

        let mut app = App::new();

        let mut branches = get_branches(&repo)?;

        if branches.is_empty() {
            write!(
                app.stdout,
                "{}\r\n",
                style("Found no branches (master is ignored)")
                    .with(Color::Yellow)
                    .attribute(Attribute::Dim)
            )?;
        } else {
            for branch in &mut branches {
                act_on_branch(branch, &mut app)?;
            }
        }

        Ok(())
    })();

    terminal::disable_raw_mode().ok();

    match result {
        Ok(()) => {}
        Err(error) => {
            eprintln!("{}", error);
            std::process::exit(1);
        }
    }
}

fn act_on_branch(branch: &mut Branch, app: &mut App) -> Result<()> {
    if branch.is_head {
        let head_message = style(format!(
            "Ignoring '{}' because it is the current branch",
            branch.name
        ))
        .with(Color::Yellow)
        .attribute(Attribute::Dim);
        write!(app.stdout, "{}\r\n", head_message)?;
    } else {
        match get_branch_action_from_user(app, &branch)? {
            BranchAction::Quit => return Ok(()),
            BranchAction::Keep => {}
            BranchAction::Delete => {
                branch.delete()?;
                let message = format!(
                    "Deleted branch '{}', to undo run `git branch {} {}`",
                    branch.name, branch.name, branch.id
                );

                let styled_message = style(message).with(Color::Yellow).attribute(Attribute::Dim);

                write!(app.stdout, "{}\r\n", styled_message)?;
            }
        }
    }
    Ok(())
}

fn get_branch_action_from_user(app: &mut App, branch: &Branch) -> Result<BranchAction> {
    let branch_name = style(format!("'{}'", branch.name)).with(Color::Green);
    let commit_hash =
        style(format!("({})", &branch.id.to_string()[0..10])).attribute(Attribute::Dim);
    let commit_time = style(format!("{}", branch.time)).with(Color::Green);
    let commands = style("(k/d/q/?)").attribute(Attribute::Bold);

    write!(
        app.stdout,
        "{} {} last commit at {} {} > ",
        branch_name, commit_hash, commit_time, commands
    )?;
    app.stdout.flush()?;

    let byte = match app.stdin.next() {
        Some(byte) => byte?,
        None => return get_branch_action_from_user(app, branch),
    };

    let c = char::from(byte);
    write!(app.stdout, "{}\r\n", c)?;

    if c == '?' {
        write!(app.stdout, "\r\n")?;
        write!(
            app.stdout,
            "{}\r\n",
            style("Here are what the commands mean:").attribute(Attribute::Dim)
        )?;
        write!(
            app.stdout,
            "{} - Keep the branch\r\n",
            style("k").attribute(Attribute::Bold)
        )?;
        write!(
            app.stdout,
            "{} - Delete the branch\r\n",
            style("d").attribute(Attribute::Bold)
        )?;
        write!(
            app.stdout,
            "{} - Quit\r\n",
            style("q").attribute(Attribute::Bold)
        )?;
        write!(
            app.stdout,
            "{} - Show this help text\r\n",
            style("?").attribute(Attribute::Bold)
        )?;
        write!(app.stdout, "\r\n")?;
        app.stdout.flush()?;
        get_branch_action_from_user(app, branch)
    } else {
        BranchAction::try_from(c)
    }
}

fn get_branches(repo: &Repository) -> Result<Vec<Branch>> {
    let mut brances = repo
        .branches(Some(BranchType::Local))?
        .map(|branch| -> Result<_> {
            let (branch, _) = branch?;
            let name = String::from_utf8(branch.name_bytes()?.to_vec())?;

            let commit = branch.get().peel_to_commit()?;

            let time = commit.time();
            let offset = Duration::minutes(i64::from(time.offset_minutes()));
            let time = NaiveDateTime::from_timestamp(time.seconds(), 0) + offset;

            Ok(Branch {
                id: commit.id(),
                time,
                name,
                is_head: branch.is_head(),
                branch,
            })
        })
        .filter(|branch| {
            if let Ok(branch) = branch {
                branch.name != "master"
            } else {
                true
            }
        })
        .collect::<Result<Vec<_>>>()?;

    brances.sort_unstable_by_key(|branch| branch.time);

    Ok(brances)
}

struct App {
    stdin: Bytes<Stdin>,
    stdout: Stdout,
}

impl App {
    fn new() -> App {
        App {
            stdin: io::stdin().bytes(),
            stdout: io::stdout(),
        }
    }
}

struct Branch<'repo> {
    id: Oid,
    time: NaiveDateTime,
    name: String,
    is_head: bool,
    branch: git2::Branch<'repo>,
}

impl<'repo> Branch<'repo> {
    fn delete(&mut self) -> Result<()> {
        self.branch.delete().map_err(From::from)
    }
}

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error(transparent)]
    CrosstermError(#[from] crossterm::ErrorKind),

    #[error(transparent)]
    IoError(#[from] io::Error),

    #[error(transparent)]
    GitError(#[from] git2::Error),

    #[error(transparent)]
    FromUtf8Error(#[from] FromUtf8Error),

    #[error("Invalid input, Don't know what '{0}' means")]
    InvalidInput(char),
}

enum BranchAction {
    Keep,
    Delete,
    Quit,
}

impl TryFrom<char> for BranchAction {
    type Error = Error;

    fn try_from(value: char) -> Result<Self, Self::Error> {
        match value {
            'k' => Ok(BranchAction::Keep),
            'd' => Ok(BranchAction::Delete),
            'q' => Ok(BranchAction::Quit),
            _ => Err(Error::InvalidInput(value)),
        }
    }
}
