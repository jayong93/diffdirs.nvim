use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

use nvim_oxi::{
    self,
    api::{
        self,
        opts::{CmdOpts, CreateCommandOpts},
        types::{CmdInfos, CommandArgs, CommandModifiers, CommandNArgs},
    },
    print, Dictionary, Function, Object,
};

#[nvim_oxi::plugin]
fn diffdirs() -> nvim_oxi::Result<Dictionary> {
    Ok(Dictionary::from_iter([(
        "config",
        Function::from_fn(config),
    )]))
}

fn config(_: Object) {
    api::create_user_command(
        "DiffDirs",
        show_diff,
        &CreateCommandOpts::builder()
            .desc("Show diff for two directories")
            .nargs(CommandNArgs::OneOrMore)
            .build(),
    )
    .map_err(|err| print!("failed to register command DiffDirs: {err}"))
    .ok();
}

fn show_diff(args: CommandArgs) {
    let cmd_args = &args.fargs;
    match cmd_args.as_slice() {
        [left_dir, right_dir] => {
            let left_dir = Path::new(left_dir);
            let right_dir = Path::new(right_dir);
            let files = make_file_set(left_dir, right_dir);
            for file in files {
                show_diff_tab(&left_dir.join(&file), &right_dir.join(&file))
                    .map_err(|err| print!("error: failed to show diff tab: {err}"))
                    .ok();
            }
        }
        _ => {
            print!("error: the number of arguments for 'DiffDirs' command wasn't 2");
        }
    }
}

fn collect_file_paths(dir: &Path) -> BTreeSet<PathBuf> {
    WalkDir::new(dir)
        .into_iter()
        .filter_map(|entry| {
            match entry.map_err(|err| err.to_string()).and_then(|e| {
                if e.file_type().is_file() {
                    e.path()
                        .strip_prefix(dir)
                        .map_err(|err| err.to_string())
                        .map(|path| Some(path.to_owned()))
                } else {
                    Ok(None)
                }
            }) {
                Ok(path) => path,
                Err(err) => {
                    print!(
                        "error occurred during walking dir: {}. err: {}",
                        dir.to_string_lossy(),
                        err
                    );
                    None
                }
            }
        })
        .collect()
}

fn make_file_set(left_dir: &Path, right_dir: &Path) -> BTreeSet<PathBuf> {
    let mut file_set: BTreeSet<PathBuf> = collect_file_paths(left_dir);
    file_set.extend(collect_file_paths(right_dir));
    file_set
}

fn show_diff_tab(left_file: &Path, right_file: &Path) -> Result<(), api::Error> {
    let cmd_opt = CmdOpts::builder().output(false).build();
    let new_tab_cmd = CmdInfos::builder()
        .cmd("tabedit")
        .args([left_file.to_string_lossy()])
        .nextcmd("difft")
        .build();
    let mut command_mod = CommandModifiers::default();
    command_mod.vertical = true;
    let diff_tab_cmd = CmdInfos::builder()
        .cmd("diffs")
        .args([right_file.to_string_lossy()])
        .mods(command_mod)
        .build();
    api::cmd(&new_tab_cmd, &cmd_opt)?;
    api::cmd(&diff_tab_cmd, &cmd_opt)?;
    Ok(())
}
