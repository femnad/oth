use std::env;
use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::process::Command;

use clap::Parser;
use git2::{Repository, Status, StatusOptions};
use regex::Regex;
use skim::prelude::*;

const DEFAULT_EDITOR: &str = "nvim";
const DEFAULT_REMOTE: &str = "origin";

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    remote_diff: bool,
    #[arg(short, long)]
    upstream_diff: bool,
    #[arg(short, long)]
    editor: Option<String>,
}

fn get_default_branch(workdir_path: &Path) -> Option<String> {
    let remote_head = workdir_path.join(format!(".git/refs/remotes/{}/HEAD", DEFAULT_REMOTE));
    let content = fs::read_to_string(remote_head).unwrap();
    let ref_line = content.trim();
    let regex = Regex::new(format!("ref: refs/remotes/{}/(.*)", DEFAULT_REMOTE).as_str()).unwrap();
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

fn has_staged_files(repo: &Repository) -> bool {
    let mut opts = StatusOptions::new();
    opts.show(git2::StatusShow::Index);
    let statuses = repo.statuses(Some(&mut opts)).unwrap();
    !statuses.is_empty()
}

fn main() {
    let args = Args::parse();

    let repo = Repository::discover(".").unwrap();
    let workdir = repo.workdir().unwrap();

    let default_branch_name = get_default_branch(&workdir).unwrap();
    let mut staged_changes = false;

    let diff = if args.upstream_diff {
        let head = repo.head().unwrap();
        let local_branch = git2::Branch::wrap(head);
        let upstream_branch = local_branch.upstream().unwrap();

        let local_tree = local_branch.get().peel_to_tree().unwrap();
        let upstream_tree = upstream_branch.get().peel_to_tree().unwrap();
        println!("{}", upstream_branch.name().unwrap().unwrap().to_owned());
        repo.diff_tree_to_tree(Some(&upstream_tree), Some(&local_tree), None)
            .unwrap()
    } else if args.remote_diff {
        let default_obj = repo.revparse_single(default_branch_name.as_str()).unwrap();
        let default_tree = default_obj.peel_to_tree().unwrap();
        let local_tree = repo.head().unwrap().peel_to_tree().unwrap();
        repo.diff_tree_to_tree(Some(&default_tree), Some(&local_tree), None)
            .unwrap()
    } else if has_staged_files(&repo) {
        staged_changes = true;
        let head_tree = repo.head().unwrap().peel_to_tree().unwrap();
        repo.diff_tree_to_index(Some(&head_tree), None, None).unwrap()
    } else {
        repo.diff_tree_to_workdir(None, None).unwrap()
    };
    let mut file_names = Vec::new();

    for delta in diff.deltas() {
        if let Some(path) = delta.new_file().path() {
            file_names.push(path.to_string_lossy().into_owned())
        }
    }

    if file_names.is_empty() {
        return;
    }

    let preview = if args.remote_diff {
        format!("git diff {} --color=always {{}}", default_branch_name)
    } else if staged_changes {
        "git diff --cached --color=always {}".to_string()
    } else {
        "git diff --color=always {}".to_string()
    };

    let options = SkimOptionsBuilder::default()
        .multi(true)
        .preview(Some(preview))
        .build()
        .unwrap();

    let item_reader = SkimItemReader::default();
    let items = item_reader.of_bufread(Cursor::new(file_names.join("\n")));
    let skim_out = Skim::run_with(&options, Some(items)).unwrap();

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
