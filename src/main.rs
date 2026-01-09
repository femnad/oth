use std::env;
use std::io::Cursor;
use std::process::Command;
use git2::Repository;
use skim::prelude::*;

fn main() {
    let repo = Repository::init(".").unwrap();
    let diff = repo.diff_index_to_workdir(None, None).unwrap();
    let mut file_names = Vec::new();

    for delta in diff.deltas() {
        if let Some(path) = delta.new_file().path() {
            file_names.push(path.to_string_lossy().into_owned())
        }
    }

    let options = SkimOptionsBuilder::default()
        .multi(true)
        .preview(Some("git diff --color=always {}".to_string()))
        .build()
        .unwrap();

    let item_reader = SkimItemReader::default();
    let items = item_reader.of_bufread(Cursor::new(file_names.join("\n")));
    let skim_out = Skim::run_with(&options, Some(items)).unwrap();

    if skim_out.is_abort {
    }

    for item in skim_out.selected_items {
        let editor = env::var("EDITOR").unwrap();
        // get arg for each item
        Command::new(editor).arg(item.output().as_ref()).status().unwrap();
    }
}