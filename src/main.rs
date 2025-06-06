mod auto_linux;
use auto_linux::UpdateMarker as upd_marker;

use std::env::{current_dir, home_dir};
use std::fs::{self, read_dir};
use std::io::{BufRead, BufReader,Write};
use std::path::{Path, PathBuf};
use tokio::process::Command;
use std::str::from_utf8;
use std::sync::LazyLock;
use chrono::{Local, NaiveDate};


static LOG_FILE: LazyLock<PathBuf> = LazyLock::new(|| {
    let mut dir = current_dir()
        .expect("should be log dir")
        .join(env!("CARGO_PKG_NAME"));
    dir.set_extension("log");
    dir
});

static CURRENT_EXEC_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    // path for works with
    let path = current_dir().unwrap().join(env!("CARGO_PKG_NAME"));
    //
    path
});



// create collection paths with nested dirs -> local repository
fn dirs(path: &PathBuf) -> Vec<PathBuf> {
    let main_path = read_dir(path)
        .expect("missing directory")
        .map(|res|res.map(|f| f.path()))
        .collect::<Vec<_>>();

    let mut nested_dirs = vec![];
    for i in main_path {
        let res = match &i {
            Ok(i) if !i.is_dir() => {continue},
            _ => {i.unwrap()}
        };

        for dir in read_dir(res).unwrap() {
            nested_dirs.push(dir.unwrap().path())
        }
    }
    nested_dirs
}

async fn write_logs(data: &[u8]) {
    let log_file = &*LOG_FILE;
    let _ = fs::File::options()
        //.write(true)
        .append(true)
        .open(log_file)
        .expect("error log writes")
        .write(data);
}



struct UpdLocGit {
    nested_dirs: Vec<PathBuf>,
}

impl UpdLocGit {
    async fn init() -> Self {
        // path for works with
        let path = current_dir().unwrap();
        //
        let nested_dirs = dirs(&path);
        Self {
            nested_dirs,
        }
    }
    async fn check_interval(&self,) -> Option<bool> {
        let log_file = &*LOG_FILE;
        assert!(log_file.exists(), "log file don't exist");
        
        let file = fs::File::open(log_file)
            .expect("it seems log file absent for interval method");
        let buf_reader = BufReader::new(file);
        let mut  old_date = String::new();
        let mut interval = String::new();
        for i in buf_reader.lines() {
            match i.unwrap() {
                e if e.contains("20") => {
                    let date= e.split_at(10).0;
                    old_date.push_str(date);//.expect("error when write from log_file date");
                }
                e if e.contains("update") => {
                    let inter = e.split_at(21).1;
                    interval.push_str(inter);//.expect("error when write from log_file interval");
                }
                _ => {}
            }
        };
        
        // if interval from log file is 0 or "" return None
        if interval.contains("0") | interval.is_empty() {
            // remove autoupdate from config file
            upd_marker.remove();
            return None;
        }
        
        let exec_path_marker = &*CURRENT_EXEC_PATH;
        if !upd_marker.is_exist()
            .expect("error checking existing marker") { 
            upd_marker.create(exec_path_marker)
        }
        
        // parse old date from log file
        let interval = interval.parse::<i64>().expect("parse interval to i64");
        let old_date = old_date
            .parse::<NaiveDate>()
            .expect("error parsing old date");
        // parse current date
        let current_date = self.time_now()
            .split_at(10).0
            .parse::<NaiveDate>()
            .expect("problem parsing current date");
        let past_days = current_date.signed_duration_since(old_date).num_days();
        
        // check conditions
        if past_days > interval {
            Some(true)
        } else { Some(false) }
    }
    fn time_now(&self) -> String {
        let time = chrono::Utc::now().with_timezone(&Local);
        let mut time = time.format("%Y-%m-%d %H:%M:%S").to_string();
        time.push('\n');
        time
    }
    fn create_log_file(&self) {
        let log_file = &*LOG_FILE.as_path();
        fs::File::create(log_file)
            .expect("something wrong with creating log_file");
    }
    async fn clear_log_file(&self) {
        let log_file = &*LOG_FILE.as_path();
        tokio::fs::File::options()
            .write(true)
            .truncate(true)
            .open(log_file)
            .await
            .expect("prob with clear log");
    }
    async fn tasks(self) {
        let git_pull = async move |dir: PathBuf| Command::new("git")
            .current_dir(dir)
            .args(["pull"])
            .output()
            .await
            .expect("error git fetch");

        let git_stash = async move |dir: PathBuf| Command::new("git")
            .current_dir(dir)
            .args(["stash"])
            .output()
            .await
            .expect("error git fetch");

        let git_url = async move |dir:PathBuf| Command::new("git")
            .current_dir(&dir)
            .args(["ls-remote", "--get-url"])
            .output()
            .await
            .expect("comand failed");

        // create all our task for parallel execution
        let mut tasks = Vec::with_capacity(self.nested_dirs.len());
        for i in self.nested_dirs {
            tasks.push(tokio::spawn(async move {
                let out = git_url(i.clone()).await;
                // check if our repository are exists
                if !out.status.success() {
                    let path = i.to_str().unwrap().as_bytes();
                    write_logs([path, b" -> ", out.stderr.as_slice()].concat().as_slice()).await;
                } else {
                    // pull request to update
                    let pull = git_pull(i.clone()).await;

                    //tokio::time::sleep(Duration::from_secs(10)).await;
                    let da = pull.stdout.as_slice();

                    let result = from_utf8(da)
                        .unwrap()
                        .lines()
                        .find(|l| l.contains("Получение обьектов")
                            | l.contains("Уже актуально")
                            | l.contains("error") | l.contains("Обновление"));

                    match result {
                        Some(a) if a.contains("Уже актуально") => {
                            let data = result.unwrap().as_bytes();
                            write_logs([data, b" -> ",out.stdout.as_slice()].concat().as_slice()).await;
                        }
                        Some(a) if a.contains("Получение обьектов") => {
                            let data = result.unwrap().as_bytes();
                            write_logs([data, b" -> ",out.stdout.as_slice()].concat().as_slice()).await;
                        }
                        Some(a) if a.contains("error") => {
                            let data = result.unwrap().as_bytes();
                            write_logs([data, b" -> ",out.stdout.as_slice()].concat().as_slice()).await;
                        }
                        Some(a) if a.contains("Обновление") => {
                            let data = result.unwrap().as_bytes();
                            write_logs([data, b" -> ",out.stdout.as_slice()].concat().as_slice()).await;
                        }

                        None => {
                            // if problems with update repo run stash - pull
                            git_stash(i.clone()).await;
                            let pull  = git_pull(i.clone()).await;
                            let da = pull.stdout.as_slice();

                            let result = from_utf8(da)
                                .unwrap()
                                .lines()
                                .find(|l| l.contains("Получение обьектов")
                                    | l.contains("Уже актуально")
                                    | l.contains("error") | l.contains(""));

                            // problem with update local repo error will send to log file
                            if result.is_none() {
                                let res = pull.stderr.as_slice();
                                let result = res
                                    .lines()
                                    .find(|l| l.as_ref().unwrap().contains("."))
                                    .unwrap();

                                let data = result.unwrap();
                                write_logs([data.as_bytes(), b" -> ",out.stdout.as_slice()].concat().as_slice()).await;
                                // remove folder, create new local from remote repo

                            } else {
                                let data = result.unwrap().as_bytes();
                                write_logs([data, b" -> ",out.stdout.as_slice()].concat().as_slice()).await;
                            }
                        }
                        _ => {}
                    }
                }
            }));
        }
        let mut outputs = Vec::with_capacity(tasks.len());
        for task in tasks {
            outputs.push(task.await);
        }
    }
}

#[tokio::main]
async fn main() {
    // set interval for log file in days
    let interval = "update_interval_days=20\n".as_bytes();
    let update = UpdLocGit::init().await;
    let exec_path_marker = &*CURRENT_EXEC_PATH;
    let log_file = &*LOG_FILE;
    
    if !log_file.exists() {
        update.create_log_file();
        assert!(log_file.exists(), "log file don't exist in main");
        write_logs(update.time_now().as_bytes()).await;
        write_logs(interval).await;
        // create marker for autoupdate
        upd_marker.create(exec_path_marker);
        let _ = update.tasks().await;
    } else { 
        let check_interval = update.check_interval().await;
        match check_interval {
            Some(t) if t.eq(&true) => {
                update.clear_log_file().await;
                write_logs(update.time_now().as_bytes()).await;
                write_logs(interval).await;
                let _ = update.tasks().await;
            }
            Some(t) if t.eq(&false) => {}
            _ => {
                // local git updates when we run file
                let none_interval = "update_interval_days=0\n".as_bytes();
                update.clear_log_file().await;
                write_logs(update.time_now().as_bytes()).await;
                write_logs(none_interval).await;
                let _ = update.tasks().await;
            }
        }
    }
    
}

#[cfg(test)]
mod tests {
    use std::time::Duration;
    use tokio::net::UdpSocket;
    use super::*;
    #[tokio::test]
    async fn log_dir() {
        let log_f = &*LOG_FILE;
        let upd_git = UpdLocGit::init().await;
        upd_git.create_log_file();
        assert!(log_f.exists(), "log file don't exist");
        
    }
    #[tokio::test]
    async fn true_interval() {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let some_data = "testing data for writes to log file\n".as_bytes();
        let interval_data = "update_interval_days=16\n".as_bytes();
        let old_data = "2025-05-06 15:51:26\n".as_bytes();
        write_logs(old_data).await;
        write_logs(interval_data).await;
        write_logs(some_data).await;
    }
    #[tokio::test]
    async fn check_interval_true()  {
        tokio::time::sleep(Duration::from_millis(800)).await;
        let upd_git = UpdLocGit::init().await;
        let true_interval= upd_git.check_interval().await.expect("true interval error");
        // last interval was 16 days
        // last time updated 2025-05-06 15:51:26
        // current time now => if now - last_time > last_interval true_interval return true 
        assert!(true_interval, "it is wasn't true");
    }

    #[tokio::test]
    async fn none_interval() {
        tokio::time::sleep(Duration::from_millis(1000)).await;
        let upd_git = UpdLocGit::init().await;
        upd_git.clear_log_file().await;
        let some_data = "testing data for writes to log file\n".as_bytes();
        let interval_data = "update_interval_days=\n".as_bytes();
        let old_data = "2025-05-06 15:51:26\n".as_bytes();
        write_logs(old_data).await;
        write_logs(interval_data).await;
        write_logs(some_data).await;
    }
    #[tokio::test]
    async fn check_interval_none()  {
        tokio::time::sleep(Duration::from_millis(1200)).await;
        let upd_git = UpdLocGit::init().await;
        let true_interval= upd_git.check_interval().await;
        // if intervals is 0 or is_empty return none
        assert_eq!(true_interval, None)
    }

}
