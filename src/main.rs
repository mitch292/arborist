use std::io;
use std::io::{Read, Write, Stdout, Stdin, Bytes};
use std::convert::TryFrom;
use std::string::FromUtf8Error;
use git2::{BranchType, Oid, Repository};
use chrono::prelude::*;
use chrono::Duration;

// TODO: 
//  1. Color the output to the terminal
//  2. Option to delete remote branches
//  3. Create an App struct where stdout and stdin live

type Result<T, E = Error> = std::result::Result<T, E>;

fn main() {
    let result = (|| -> Result<_> {
        let repo = Repository::open_from_env()?;
        crossterm::terminal::enable_raw_mode()?;
    
        let mut stdout = io::stdout();
        let mut stdin = io::stdin().bytes();

        let mut branches = get_branches(&repo)?;

        if branches.is_empty() {
            write!(stdout, "Found no branches (master is ignored)\r\n")?;
        } else {
            for branch in &mut branches {
                act_on_branch(branch, &mut stdout, &mut stdin)?;
            }
        }

       Ok(())

    })();

    crossterm::terminal::disable_raw_mode().ok();

    match result {
        Ok(()) => {}
        Err(error) => {
            eprintln!("{}", error);
            std::process::exit(1);
        }
    }
}

fn act_on_branch(
    branch: &mut Branch,
    stdout: &mut Stdout,
    stdin: &mut Bytes<Stdin>
) -> Result<()> {
    if branch.is_head {
        write!(
            stdout,
            "Ignoring '{}' because it is the current branch\r\n",
            branch.name
        )?;
    } else {
        match get_branch_action_from_user(stdout, stdin, &branch)? {
            BranchAction::Quit => return Ok(()),
            BranchAction::Keep => {},
            BranchAction::Delete => {
                branch.delete()?;
                write!(
                    stdout, 
                    "Deleted branch '{}', to undo run `git branch {} {}`\r\n",
                    branch.name, branch.name, branch.id
                )?;
            },
        }
    }
    Ok(())
}

fn get_branch_action_from_user(
    stdout: &mut Stdout, 
    stdin: &mut Bytes<Stdin>, 
    branch: &Branch
) -> Result<BranchAction> {
    write!(
        stdout, 
        "'{}' ({}) last commit at {} (k/d/q/?) > ",
        branch.name, &branch.id.to_string()[0..10], branch.time
    )?;
    stdout.flush()?;

    let byte = match stdin.next() {
        Some(byte) => byte?,
        None => return get_branch_action_from_user(stdout, stdin, branch),
    };

    let c = char::from(byte);
    write!(stdout, "{}\r\n", c)?;

    if c == '?' {
        write!(stdout, "Here are what the commands mean:\r\n")?;
        write!(stdout, "k - Keep the branch\r\n")?;
        write!(stdout, "d - Delete the branch\r\n")?;
        write!(stdout, "q - Quit\r\n")?;
        write!(stdout, "? - Show this help text\r\n")?;
        stdout.flush()?;
        get_branch_action_from_user(stdout, stdin, branch)
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

struct Branch<'repo> {
    id: Oid,
    time: NaiveDateTime,
    name: String,
    is_head: bool,
    branch: git2::Branch<'repo>,
}

impl <'repo> Branch<'repo> {
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
    InvalidInput(char)
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
            _ => Err(Error::InvalidInput(value))
        }
    }
}