use std::path::PathBuf;

pub fn configure_data_dir(data_dir: Option<PathBuf>) {
    crate::storage::set_data_dir(data_dir);
}
