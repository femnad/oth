use std::env;
use std::io::Cursor;
use std::process::Command;

use clap::Parser;
use git2::{Cred, Repository};
use skim::prelude::*;

const AUTH_SOCK_VAR: &str = "SSH_AUTH_SOCK";
const DEFAULT_EDITOR: &str = "nvim";

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    auth_sock: Option<String>,
    #[arg(short, long)]
    remote_diff: bool,
    #[arg(short, long)]
    upstream_diff: bool,
    #[arg(short, long)]
    editor: Option<String>,
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

fn main() {
    let args = Args::parse();
    let repo = Repository::init(".").unwrap();

    let mut default_branch_name = String::new();
    let diff = if args.upstream_diff {
        let head = repo.head().unwrap();
        let local_branch = git2::Branch::wrap(head);
        let upstream_branch = local_branch.upstream().unwrap();

        let local_tree = local_branch.get().peel_to_tree().unwrap();
        let upstream_tree = upstream_branch.get().peel_to_tree().unwrap();
        println!("{}", upstream_branch.name().unwrap().unwrap().to_owned());
        repo.diff_tree_to_tree(Some(&upstream_tree), Some(&local_tree), None).unwrap()
    } else if args.remote_diff {
        if let Some(auth_sock) = args.auth_sock {
            unsafe { env::set_var(AUTH_SOCK_VAR, auth_sock.as_str()); }
        }

        let mut remote = repo.find_remote("origin").unwrap();
        let mut callbacks = git2::RemoteCallbacks::new();
        callbacks.credentials(|_url, username_from_url, _allowed_types| {
            let user = username_from_url.unwrap();
            Cred::ssh_key_from_agent(&user)
        });

        remote.connect_auth(git2::Direction::Fetch, Some(callbacks), None).unwrap();
        let default_branch_buf = remote.default_branch().unwrap();
        default_branch_name = default_branch_buf.as_str().unwrap().to_owned();

        let default_obj = repo.revparse_single(default_branch_name.as_str()).unwrap();
        let default_tree = default_obj.peel_to_tree().unwrap();
        let local_tree = repo.head().unwrap().peel_to_tree().unwrap();

        repo.diff_tree_to_tree(Some(&default_tree), Some(&local_tree), None).unwrap()
    } else {
        repo.diff_index_to_workdir(None, None).unwrap()
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
    }

    let editor = get_editor(args.editor);
    for item in skim_out.selected_items {
        Command::new(editor.clone()).arg(item.output().as_ref()).status().unwrap();
    }
}