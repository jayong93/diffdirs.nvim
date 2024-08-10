use error::Error as DiffDirsError;
use serde::Deserialize;
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
        types::{CmdInfos, CommandArgs, CommandModifiers, CommandNArgs, Mode, SplitModifier},
        Buffer, StringOrFunction, TabPage,
    },
    print, Array, Dictionary, Function, Object,
};

mod config;
mod error;

#[derive(Debug)]
enum DiffDirType {
    Two(PathBuf, PathBuf),
    Three(PathBuf, PathBuf, PathBuf),
}

impl Default for DiffDirType {
    fn default() -> Self {
        Self::Two(PathBuf::new(), PathBuf::new())
    }
}

thread_local! {
    static DIFF_FILES: RefCell<BTreeMap<PathBuf, TabPage>> = const {RefCell::new(BTreeMap::new())};
    static DIFF_DIRS: RefCell<DiffDirType> = RefCell::new(Default::default());
    static CONFIG: RefCell<config::Config> = const {RefCell::new(config::Config::new())};
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

fn setup(config: Object) -> Result<(), DiffDirsError> {
    let de = nvim_oxi::serde::Deserializer::new(config);
    let config = config::Config::deserialize(de)?;
    CONFIG.replace(config);
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
                    let path = Path::new(&path);
                    DIFF_DIRS.with_borrow(|dirs| {
                        CONFIG.with_borrow(|config| {
                            match dirs {
                                DiffDirType::Two(left_dir, right_dir) => {
                                    TwoPaneDiff {
                                        left_dir,
                                        right_dir,
                                    }
                                    .open_diff_tab(path, "tabedit", config)?;
                                    *tab = api::get_current_tabpage();
                                }
                                DiffDirType::Three(left_dir, right_dir, output_dir) => {
                                    ThreePaneDiff {
                                        left_dir,
                                        right_dir,
                                        output_dir,
                                    }
                                    .open_diff_tab(path, "tabedit", config)?;
                                    *tab = api::get_current_tabpage();
                                }
                            };
                            Ok(())
                        })
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
    CONFIG.with_borrow(|config| match cmd_args.as_slice() {
        [left_dir, right_dir] => {
            let left_dir = Path::new(left_dir);
            let right_dir = Path::new(right_dir);
            DIFF_DIRS.replace(DiffDirType::Two(left_dir.to_owned(), right_dir.to_owned()));
            TwoPaneDiff {
                left_dir,
                right_dir,
            }
            .diff_files(config)
        }
        [left_dir, right_dir, output_dir] => {
            let left_dir = Path::new(left_dir);
            let right_dir = Path::new(right_dir);
            let output_dir = Path::new(output_dir);
            DIFF_DIRS.replace(DiffDirType::Three(
                left_dir.to_owned(),
                right_dir.to_owned(),
                output_dir.to_owned(),
            ));
            ThreePaneDiff {
                left_dir,
                right_dir,
                output_dir,
            }
            .diff_files(config)
        }
        _ => Err(DiffDirsError::other(
            "the number of arguments for 'DiffDirs' command wasn't 2",
        )),
    })
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

fn init_diff_tab(
    cmd_str: &str,
    cmd_opt: &CmdOpts,
    file: &Path,
    config: &config::Config,
) -> Result<(), DiffDirsError> {
    let new_tab_cmd = CmdInfos::builder()
        .cmd(cmd_str)
        .args([file.to_string_lossy()])
        .nextcmd("difft")
        .build();
    api::cmd(&new_tab_cmd, cmd_opt)?;
    config.set_left_diff_opt(api::get_current_win())?;
    Ok(())
}

fn split_diff_win(
    cmd_mod: &CommandModifiers,
    cmd_opt: &CmdOpts,
    file: &Path,
) -> Result<(), DiffDirsError> {
    let split_cmd = CmdInfos::builder()
        .cmd("diffs")
        .args([file.to_string_lossy()])
        .mods(*cmd_mod)
        .build();
    api::cmd(&split_cmd, cmd_opt)?;
    Ok(())
}

trait ShowDiff {
    fn base_paths(&self) -> (&Path, &Path);
    fn open_diff_tab(
        &self,
        file: &Path,
        cmd_str: &str,
        config: &config::Config,
    ) -> Result<(Buffer, PathBuf), DiffDirsError>;

    fn diff_files(&self, config: &config::Config) -> Result<(), DiffDirsError> {
        let files = self.make_file_set();

        let first_tabpage = api::get_current_tabpage();
        let mut is_first_cmd = true;
        api::call_function::<_, usize>("setqflist", (Array::new(), 'r'))?;

        let mut path_tab_map = BTreeMap::new();
        for file in files {
            let (modifiable_buf, modifiable_file) =
                self.open_diff_tab(&file, if is_first_cmd { "edit" } else { "tabedit" }, config)?;
            let mut qflist_entry = Dictionary::new();
            qflist_entry.insert("bufnr", modifiable_buf.handle());
            qflist_entry.insert("filename", modifiable_file.to_string_lossy());
            qflist_entry.insert("text", file.to_string_lossy());
            api::call_function::<_, usize>("setqflist", (Array::from_iter([qflist_entry]), 'a'))?;
            is_first_cmd = false;
            path_tab_map.insert(file, api::get_current_tabpage());
        }
        api::set_current_tabpage(&first_tabpage)?;
        DIFF_FILES.replace(path_tab_map);
        Ok(())
    }
    fn make_file_set(&self) -> BTreeSet<PathBuf> {
        let (left_dir, right_dir) = self.base_paths();
        let mut file_set: BTreeSet<PathBuf> = collect_file_paths(left_dir);
        file_set.extend(collect_file_paths(right_dir));
        file_set
    }
}

struct TwoPaneDiff<'a> {
    left_dir: &'a Path,
    right_dir: &'a Path,
}

impl<'a> ShowDiff for TwoPaneDiff<'a> {
    fn base_paths(&self) -> (&Path, &Path) {
        (self.left_dir, self.right_dir)
    }

    fn open_diff_tab(
        &self,
        file: &Path,
        cmd_str: &str,
        config: &config::Config,
    ) -> Result<(Buffer, PathBuf), DiffDirsError> {
        let cmd_opt = CmdOpts::builder().output(false).build();
        init_diff_tab(cmd_str, &cmd_opt, &self.left_dir.join(file), config)?;

        let mut cmd_mod = CommandModifiers::default();
        cmd_mod.vertical = true;
        let modifiable_file = self.right_dir.join(file);
        split_diff_win(&cmd_mod, &cmd_opt, &modifiable_file)?;
        config.set_right_diff_opt(api::get_current_win())?;
        Ok((api::get_current_buf(), modifiable_file))
    }
}

struct ThreePaneDiff<'a> {
    left_dir: &'a Path,
    right_dir: &'a Path,
    output_dir: &'a Path,
}

impl<'a> ShowDiff for ThreePaneDiff<'a> {
    fn base_paths(&self) -> (&Path, &Path) {
        (self.left_dir, self.right_dir)
    }

    fn open_diff_tab(
        &self,
        file: &Path,
        cmd_str: &str,
        config: &config::Config,
    ) -> Result<(Buffer, PathBuf), DiffDirsError> {
        let cmd_opt = CmdOpts::builder().output(false).build();
        init_diff_tab(cmd_str, &cmd_opt, &self.left_dir.join(file), config)?;

        let mut cmd_mod = CommandModifiers::default();
        cmd_mod.vertical = true;
        split_diff_win(&cmd_mod, &cmd_opt, &self.right_dir.join(file))?;
        config.set_left_diff_opt(api::get_current_win())?;

        let modifiable_file = self.output_dir.join(file);
        let mut cmd_mod = CommandModifiers::default();
        cmd_mod.split = Some(SplitModifier::BotRight);
        split_diff_win(&cmd_mod, &cmd_opt, &modifiable_file)?;
        config.set_right_diff_opt(api::get_current_win())?;
        Ok((api::get_current_buf(), modifiable_file))
    }
}
