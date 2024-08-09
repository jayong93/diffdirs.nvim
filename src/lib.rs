use derive_builder::Builder;
use error::{BuilderError, Error as DiffDirsError};
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
                    DIFF_DIRS.with_borrow(|dirs| {
                        CONFIG.with_borrow(|config| {
                            match dirs {
                                DiffDirType::Two(left_dir, right_dir) => {
                                    DiffTabBuilder::default()
                                        .left_file(&(left_dir.join(&path)))
                                        .right_file(&(right_dir.join(&path)))
                                        .config(config)
                                        .build()?
                                        .open(false)?;
                                    *tab = api::get_current_tabpage();
                                }
                                DiffDirType::Three(left_dir, right_dir, output_dir) => {
                                    DiffTabBuilder::default()
                                        .left_file(&(left_dir.join(&path)))
                                        .right_file(&(right_dir.join(&path)))
                                        .output_file(output_dir.join(&path))
                                        .config(config)
                                        .build()?
                                        .open(false)?;
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
        [left_dir, right_dir] => DiffContextBuilder::default()
            .left_dir(Path::new(left_dir))
            .right_dir(Path::new(right_dir))
            .config(config)
            .build()?
            .open_tabs(),
        [left_dir, right_dir, output_dir] => DiffContextBuilder::default()
            .left_dir(Path::new(left_dir))
            .right_dir(Path::new(right_dir))
            .output_dir(Path::new(output_dir))
            .config(config)
            .build()?
            .open_tabs(),
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

fn make_file_set(left_dir: &Path, right_dir: &Path) -> BTreeSet<PathBuf> {
    let mut file_set: BTreeSet<PathBuf> = collect_file_paths(left_dir);
    file_set.extend(collect_file_paths(right_dir));
    file_set
}

#[derive(Debug, Builder)]
#[builder(build_fn(error = "BuilderError"))]
struct DiffContext<'a, 'b> {
    left_dir: &'a Path,
    right_dir: &'a Path,
    #[builder(default, setter(into, strip_option))]
    output_dir: Option<&'a Path>,
    config: &'b config::Config,
}

impl<'a, 'b> DiffContext<'a, 'b> {
    fn open_tabs(&self) -> Result<(), DiffDirsError> {
        if let Some(output_dir) = self.output_dir {
            DIFF_DIRS.replace(DiffDirType::Three(
                self.left_dir.to_owned(),
                self.right_dir.to_owned(),
                output_dir.to_owned(),
            ));
        } else {
            DIFF_DIRS.replace(DiffDirType::Two(
                self.left_dir.to_owned(),
                self.right_dir.to_owned(),
            ));
        }
        let files = make_file_set(self.left_dir, self.right_dir);

        let first_tabpage = api::get_current_tabpage();
        let mut is_first_cmd = true;
        api::call_function::<_, usize>("setqflist", (Array::new(), 'r'))?;

        let mut path_tab_map = BTreeMap::new();
        for file in files {
            let left_file = self.left_dir.join(&file);
            let right_file = self.right_dir.join(&file);
            let mut tab_builder = DiffTabBuilder::default();
            tab_builder
                .config(self.config)
                .left_file(&left_file)
                .right_file(&right_file);
            let modifiable_file = if let Some(output_dir) = self.output_dir {
                let output_file = output_dir.join(&file);
                tab_builder.output_file(output_file.clone());
                output_file
            } else {
                right_file.clone()
            };
            tab_builder.build()?.open(is_first_cmd)?;
            let modifiable_buf = Buffer::current();
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
}

#[derive(Debug, Builder)]
#[builder(build_fn(error = "BuilderError"))]
struct DiffTab<'a, 'b> {
    left_file: &'a Path,
    right_file: &'a Path,
    #[builder(default, setter(into, strip_option))]
    output_file: Option<PathBuf>,
    config: &'b config::Config,
}

impl<'a, 'b> DiffTab<'a, 'b> {
    fn open(&self, is_first: bool) -> Result<(), DiffDirsError> {
        let cmd_opt = CmdOpts::builder().output(false).build();
        let new_tab_cmd_str = if is_first { "edit" } else { "tabedit" };
        let new_tab_cmd = CmdInfos::builder()
            .cmd(new_tab_cmd_str)
            .args([self.left_file.to_string_lossy()])
            .nextcmd("difft")
            .build();
        let mut command_mod = CommandModifiers::default();
        command_mod.vertical = true;
        let diff_win_cmd = CmdInfos::builder()
            .cmd("diffs")
            .args([self.right_file.to_string_lossy()])
            .mods(command_mod)
            .build();

        api::cmd(&new_tab_cmd, &cmd_opt)?;
        self.set_left_opt()?;

        api::cmd(&diff_win_cmd, &cmd_opt)?;
        if let Some(output) = &self.output_file {
            self.set_left_opt()?;

            let mut cmd_mod = CommandModifiers::default();
            cmd_mod.split = Some(SplitModifier::BotRight);
            let output_win_cmd = CmdInfos::builder()
                .cmd("diffs")
                .args([output.to_string_lossy()])
                .mods(cmd_mod)
                .build();
            api::cmd(&output_win_cmd, &cmd_opt)?;
        }
        self.set_right_opt()?;
        Ok(())
    }

    #[inline]
    fn set_left_opt(&self) -> Result<(), DiffDirsError> {
        api::command("set winfixbuf | set nomodifiable")?;
        self.config.set_left_diff_opt(api::get_current_win())
    }

    #[inline]
    fn set_right_opt(&self) -> Result<(), DiffDirsError> {
        api::command("set winfixbuf | set modifiable")?;
        self.config.set_right_diff_opt(api::get_current_win())
    }
}
