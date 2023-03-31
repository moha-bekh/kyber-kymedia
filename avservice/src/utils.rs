use std::path::{Path, PathBuf};

use crate::{Error, Result};

pub(crate) fn get_resource_path(resource_name: &str) -> PathBuf {
    if let Ok(mut path) = std::env::current_exe() {
        path.pop();
        path.push(resource_name);

        path
    } else {
        PathBuf::from(resource_name)
    }
}

pub(crate) fn path_to_string(path: &Path) -> Result<String> {
    path.to_path_buf()
        .into_os_string()
        .into_string()
        .map_err(|s| Error::OsStringConversion { s })
}

pub(crate) fn get_resource_path_str(resource_name: &str) -> Result<String> {
    path_to_string(&get_resource_path(resource_name))
}
