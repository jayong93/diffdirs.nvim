use error::Error as DiffDirsError;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

use nvim_oxi::{
    self,
    api::{
        self,
        opts::{CmdOpts, CreateCommandOpts, SetKeymapOpts},
        types::{CmdInfos, CommandArgs, CommandModifiers, CommandNArgs, Mode},
        Buffer, StringOrFunction, TabPage,
    },
    print, Array, Dictionary, Function, Object,
};

mod error;

thread_local! {
    static DIFF_FILES: RefCell<BTreeMap<PathBuf, TabPage>> = const {RefCell::new(BTreeMap::new())};
    static DIFF_DIRS: RefCell<(PathBuf, PathBuf)> = RefCell::new(Default::default());
}

#[nvim_oxi::plugin]
fn diffdirs() -> nvim_oxi::Result<Dictionary> {
    let setup_fn: Function<Object, Result<(), DiffDirsError>> = Function::from_fn(setup);
    let jumb_tab_fn: Function<String, Result<(), DiffDirsError>> =
        Function::from_fn(jump_to_diff_tab);
    Ok(Dictionary::from_iter([
        ("setup", setup_fn.to_object()),
        ("diff_files", Function::from_fn(diff_files).to_object()),
        ("jump_diff_tab", jumb_tab_fn.to_object()),
    ]))
}

fn setup(_: Object) -> Result<(), DiffDirsError> {
    Ok(api::create_user_command(
        "DiffDirs",
        |args| -> Result<(), DiffDirsError> {
            setup_keymap()?;
            show_diff(args)?;
            Ok(())
        },
        &CreateCommandOpts::builder()
            .desc("Show diff for two directories")
            .nargs(CommandNArgs::OneOrMore)
            .build(),
    )?)
}

fn diff_files(_: ()) -> Vec<String> {
    DIFF_FILES.with_borrow(|files| {
        files
            .keys()
            .map(|p| p.to_string_lossy().into_owned())
            .collect()
    })
}

fn jump_to_diff_tab(path: String) -> Result<(), DiffDirsError> {
    DIFF_FILES.with_borrow_mut(|files| {
        files
            .get_mut(<str as AsRef<Path>>::as_ref(&path))
            .ok_or_else(|| DiffDirsError::other(format!("invalid diff path: {path}")))
            .and_then(|tab| {
                if tab.is_valid() {
                    Ok(api::set_current_tabpage(tab)?)
                } else {
                    DIFF_DIRS.with_borrow(|(left_dir, right_dir)| {
                        show_diff_tab(&left_dir.join(&path), &right_dir.join(&path), false)?;
                        *tab = api::get_current_tabpage();
                        Ok(())
                    })
                }
            })
    })
}

fn setup_keymap() -> Result<(), DiffDirsError> {
    api::command("set switchbuf+=usetab")?;
    api::set_keymap(
        Mode::Normal,
        "<Plug>PrevDiff",
        ":silent cp!<cr>",
        &SetKeymapOpts::builder()
            .desc("Previous diff tab")
            .noremap(true)
            .silent(true)
            .build(),
    )?;
    api::set_keymap(
        Mode::Normal,
        "<Plug>NextDiff",
        ":silent cn!<cr>",
        &SetKeymapOpts::builder()
            .desc("Next diff tab")
            .noremap(true)
            .silent(true)
            .build(),
    )?;
    Ok(())
}

fn show_diff(args: CommandArgs) -> Result<(), DiffDirsError> {
    let cmd_args = &args.fargs;
    match cmd_args.as_slice() {
        [left_dir, right_dir] => {
            let left_dir = Path::new(left_dir);
            let right_dir = Path::new(right_dir);
            DIFF_DIRS.replace((left_dir.to_owned(), right_dir.to_owned()));
            let files = make_file_set(left_dir, right_dir);

            let first_tabpage = api::get_current_tabpage();
            let mut is_first_cmd = true;
            api::call_function::<_, usize>("setqflist", (Array::new(), 'r'))?;

            let mut path_tab_map = BTreeMap::new();
            for file in files {
                let left_file = left_dir.join(&file);
                let right_file = right_dir.join(&file);
                show_diff_tab(&left_file, &right_file, is_first_cmd)?;
                let right_buf = Buffer::current();
                let mut qflist_entry = Dictionary::new();
                qflist_entry.insert("bufnr", right_buf.handle());
                qflist_entry.insert("filename", right_file.to_string_lossy());
                qflist_entry.insert("text", file.to_string_lossy());
                api::call_function::<_, usize>(
                    "setqflist",
                    (Array::from_iter([qflist_entry]), 'a'),
                )?;
                is_first_cmd = false;
                path_tab_map.insert(file, api::get_current_tabpage());
            }
            api::set_current_tabpage(&first_tabpage)?;
            DIFF_FILES.replace(path_tab_map);
            Ok(())
        }
        _ => Err(DiffDirsError::other(
            "the number of arguments for 'DiffDirs' command wasn't 2",
        )),
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

fn show_diff_tab(left_file: &Path, right_file: &Path, is_first: bool) -> Result<(), DiffDirsError> {
    let cmd_opt = CmdOpts::builder().output(false).build();
    let new_tab_cmd_str = if is_first { "edit" } else { "tabedit" };
    let new_tab_cmd = CmdInfos::builder()
        .cmd(new_tab_cmd_str)
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
    api::command("set winfixbuf | set nomodifiable")?;
    api::cmd(&diff_tab_cmd, &cmd_opt)?;
    api::command("set winfixbuf | set modifiable")?;
    Ok(())
}
