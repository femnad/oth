use std::env;
use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::process::Command;

use clap::{Parser, ValueEnum};
use regex::Regex;
use shlex;
use skim::prelude::*;

const DEFAULT_EDITOR: &str = "nvim";
const REMOTE_FALLBACK: &str = "origin";

#[derive(Debug, Clone, PartialEq, Eq, ValueEnum)]
enum DiffMode {
    Branch,
    Remote,
    Revlist,
    RevlistRemote,
    Upstream,
}

#[derive(Parser, Debug)]
#[command(
  version,
  about,
  long_about = None
)]
struct Args {
    #[arg(short, long, value_enum, default_value = "revlist-remote")]
    diff_mode: DiffMode,
    #[arg(short, long)]
    editor: Option<String>,
    #[arg(long)]
    remote_override: Option<String>,
    #[arg(short, long)]
    selector: bool,
}

fn get_default_branch(remote: &String, workdir_path: &Path) -> Option<String> {
    let remote_head = workdir_path.join(format!(".git/refs/remotes/{}/HEAD", remote));
    let content = fs::read_to_string(remote_head.clone()).expect(
        format!("Could not read remote HEAD {}", remote_head.display()).as_str());
    let ref_line = content.trim();
    let regex = Regex::new(format!("ref: refs/remotes/{}/(.*)", remote).as_str()).unwrap();
    if let Some(captures) = regex.captures(ref_line) {
        return Some(captures[1].parse().unwrap());
    }
    None
}

fn get_editor(editor: Option<String>) -> String {
    if editor.is_some() {
        return editor.unwrap();
    }

    if let Some(editor) = env::var_os("EDITOR") {
        return editor.into_string().unwrap();
    }

    DEFAULT_EDITOR.to_string()
}

fn git_output(args: Vec<&str>) -> Result<String, String> {
    let cmd = args.join(" ");
    let output = Command::new("git").args(args).output().expect(
        format!("error running command {}", cmd).as_str());
    if !output.status.success() {
        return Err(String::from_utf8(output.stderr).unwrap());
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(stdout)
}

fn get_remote() -> String {
    let full_name = git_output(vec![
        "rev-parse",
        "--abbrev-ref",
        "--symbolic-full-name",
        "@{u}",
    ]);
    if full_name.is_err() {
        return REMOTE_FALLBACK.to_string();
    }
    full_name.expect("error getting remote").split('/').nth(0).unwrap().to_string()
}

fn is_staged() -> bool {
    !Command::new("git")
        .args(&["diff", "--cached", "--shortstat"])
        .output()
        .unwrap()
        .stdout
        .is_empty()
}

fn main() {
    let args = Args::parse();
    let workdir = git_output(vec!["rev-parse", "--show-toplevel"])
        .expect("error getting workdir");
    let remote = if args.remote_override.is_some() {
        args.remote_override.unwrap()
    } else {
        get_remote()
    };
    let default_branch_name = get_default_branch(&remote, workdir.as_ref()).unwrap();
    let staged_changes = is_staged();

    let mut diff_cmd = match args.diff_mode {
        DiffMode::Branch => {
            format!("diff {}", default_branch_name)
        },
        DiffMode::Upstream => {
            let branch = git_output(vec!["branch", "--show-current"])
                .expect("error getting branch");
            format!("diff {}/{}", remote, branch)
        },
        DiffMode::Remote => {
            format!("diff {}/{}", remote, default_branch_name)
        },
        DiffMode::Revlist => {
            let rev_list_count = git_output(vec!["rev-list", "--count", "HEAD",
                                                 format!("^{}", default_branch_name).as_str()])
                .expect("error getting rev list");
            format!("diff HEAD~{}", rev_list_count)
        },
        DiffMode::RevlistRemote => {
            let rev_list_count = git_output(vec!["rev-list", "--count", "HEAD",
                                                 format!("^{}", default_branch_name).as_str()])
                .expect("error getting rev list");
            format!("diff {}/HEAD~{}", remote, rev_list_count)
        },
    };
    if staged_changes {
        diff_cmd = format!("{} --cached", diff_cmd);
    }
    let files_cmd = format!("{} --name-only", diff_cmd);

    let cmd_vec = shlex::split(files_cmd.as_str()).expect("error parsing command string");
    let git_arg = cmd_vec.iter().map(|s| s.as_str()).collect::<Vec<&str>>();
    let file_names = git_output(git_arg)
        .expect("error getting file names")
        .split('\n')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect::<Vec<String>>();

    if file_names.is_empty() {
        return;
    }

    if !args.selector {
        file_names.iter().for_each(|s| {
            println!("{}", s);
        });
        return;
    }

    let preview = format!("git {} --color=always -- {{}}", diff_cmd);
    let options = SkimOptionsBuilder::default()
        .multi(true)
        .preview(Some(preview))
        .build()
        .unwrap();

    let item_reader = SkimItemReader::default();
    let items = item_reader.of_bufread(Cursor::new(file_names.join("\n")));
    let skim_out = Skim::run_with(options, Some(items)).unwrap();

    if skim_out.is_abort {
        return;
    }

    let editor = get_editor(args.editor);
    for item in skim_out.selected_items {
        Command::new(editor.clone())
            .arg(item.output().as_ref())
            .status()
            .unwrap();
    }
}
