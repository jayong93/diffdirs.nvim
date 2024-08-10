use nvim_oxi::{api::{self, Window}, Function};
use serde::Deserialize;

use crate::error::Error as DiffDirsError;

#[derive(Debug, Deserialize)]
pub struct Config {
    left_diff_opt_fn: Option<Function<Window, ()>>,
    right_diff_opt_fn: Option<Function<Window, ()>>,
}

impl Config {
    pub const fn new() -> Self {
        Self { left_diff_opt_fn: None, right_diff_opt_fn: None }
    }

    pub fn set_left_diff_opt(&self, win: Window) -> Result<(), DiffDirsError> {
        api::command("set winfixbuf | set nomodifiable")?;
        if let Some(f) = &self.left_diff_opt_fn {
            f.call(win)?;
        }
        Ok(())
    }

    pub fn set_right_diff_opt(&self, win: Window) -> Result<(), DiffDirsError> {
        api::command("set winfixbuf | set modifiable")?;
        if let Some(f) = &self.right_diff_opt_fn {
            f.call(win)?;
        }
        Ok(())
    }
}
