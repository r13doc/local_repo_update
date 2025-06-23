use std::env::{current_dir, home_dir};
use std::fs;
use std::fs::File;
use std::io::{Error, Write};
use std::path::{PathBuf};
use std::sync::LazyLock;


static CONFIG_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    let mut dir = home_dir()
        .expect("should be home dir")
        .join(".config")
        .join("autostart")
        .join(env!("CARGO_PKG_NAME"));
    dir.set_extension("desktop");
    dir
});



pub struct UpdateMarker;

impl UpdateMarker {
    pub fn init() -> Self {
        Self {}
    }
    pub fn create(&self, exec_path: &PathBuf) {

        let exec_path = exec_path.to_str().expect("error with exec_path");
        let autosave_file = &*CONFIG_PATH;
        let file = autosave_file.to_str().unwrap();
        let name_pkg = env!("CARGO_PKG_NAME");
        let text = format!(
            "[Desktop Entry]\n\
            Exec={}\n\
            Icon=None\n\
            Name={name_pkg}\n\
            Path=None\n\
            Terminal=False\n\
            Type=Application ",
            exec_path);

        let mut file = File::create(file).expect("creating file error");
        file.write_all(text.as_bytes()).expect("writing file error");
    }
    pub fn is_exist(&self) -> Result<bool, Error> {
        fs::exists(&*CONFIG_PATH)
    }
    pub fn remove(&self) {
       if self.is_exist().unwrap() {
           let path = CONFIG_PATH.as_path();
           fs::remove_file(path).unwrap()
       }
    }
}

#[cfg(test)]
mod tests {
    use std::thread::sleep;
    use std::time::Duration;
    use super::*;
    
    #[test]
    fn create_fn() {
        let update_mark = UpdateMarker::init();
        let path_exec = PathBuf::from("some_path".to_string());
        update_mark.create(&path_exec);
        let exist = update_mark.is_exist().expect("not exist!!!");
        assert!(exist);
    }

    #[test]
    fn dilate_fn() {
        sleep(Duration::from_millis(500));
        let update_mark = UpdateMarker::init();
        // removing path from folder
        update_mark.remove();
        let exist = update_mark.is_exist().expect("exist!!");
        assert!(!exist);
    }
}




