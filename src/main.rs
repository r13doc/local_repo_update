mod auto_linux;
use auto_linux::UpdateMarker as upd_marker;

use std::env::{current_dir, home_dir};
use std::fs::{self, read_dir};
use std::io::{BufRead, BufReader, Error, Write};
use std::path::{Path, PathBuf};
use tokio::process::Command;
use std::str::from_utf8;
use std::sync::LazyLock;
use std::time::Duration;
use chrono::{Local, NaiveDate};

// main path where located all git_code subfolders
const MAIN_PATH: &str = "..";

static LOG_FILE: LazyLock<PathBuf> = LazyLock::new(|| {
    let mut dir = MAIN_PATH.parse::<PathBuf>()
        .expect("error with main path")
        .join(env!("CARGO_PKG_NAME"));
    dir.set_extension("log");
    dir
});

static CURRENT_EXEC_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    let path = MAIN_PATH.parse::<PathBuf>()
        .expect("error exec path")
        .join(env!("CARGO_PKG_NAME"));
    
    path
});



async fn write_logs(data: &[u8]) {
    let log_file = &*LOG_FILE;
    let _ = fs::File::options()
        .append(true)
        .open(log_file)
        .expect("error log writes")
        .write(data);
}



struct UpdLocGit {
    nested_dirs: Vec<PathBuf>,
}


impl UpdLocGit {
    // create collection paths with nested dirs -> local repository
    // main folder -> subfolders with code base organized by topics
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
    
}

impl UpdLocGit {
    async fn init() -> Self {
        // path for works with
        let path = MAIN_PATH.parse::<PathBuf>().expect("error in init path");
        let nested_dirs = UpdLocGit::dirs(&path);
        Self {
            nested_dirs,
        }
    }
    
    fn date_interval(&self,) -> Result<(String, String), Error> {
        let log_file = &*LOG_FILE;
        assert!(log_file.exists(), "log file don't exist");
        
        let file = fs::File::open(log_file)
            .expect("it seems log file absent for interval method");
        let buf_reader = BufReader::new(file);
        let mut  old_date = String::new();
        let mut interval = String::new();
        for i in buf_reader.lines() {
            let i = i?;
            if i.contains("20") {
                let date:Vec<_> = i.split(" ").collect();
                let date = date[0];
                old_date.push_str(date);
            }
            if i.contains("update_interval_days") {
                let inter = i.split("=").last().expect("should be number");
                interval.push_str(inter);
            }
        };
        Ok((old_date, interval))
    }
    async fn check_interval(&self) -> Option<bool> {
        let(old_date, interval) = self.date_interval().expect("error old_date interval in check_interval");
        // if interval from log file is 0 or "" return None
        if interval.eq("0") | interval.is_empty() {
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
            let out = git_url(i.clone()).await;
            // check if our repository are exists
            if !out.status.success() {
                let path = i.to_str().unwrap().as_bytes();
                write_logs([path, b" -> ", out.stderr.as_slice()].concat().as_slice()).await;
                let empty = tokio::spawn(async { });
                tasks.push(empty);
            } else {
                let task = tokio::spawn(async move {
                    loop {
                        // pull request to update
                        let pull = git_pull(i.clone()).await;
                        let da = pull.stdout.as_slice();
                        let res = from_utf8(da)
                            .expect("data reading error")
                            .lines()
                            .find(|l| l.contains("Уже актуально") | l.contains("error"));
                        
                        if res.is_none() {
                            tokio::time::sleep(Duration::from_secs(2)).await;
                            continue;
                        }
                        if res.unwrap().contains("Уже актуально") {
                            let data = "Актуально ".as_bytes();
                            write_logs([data, b" -> ", out.stdout.as_slice()].concat().as_slice()).await;
                            break
                        }
                        if res.unwrap().contains("error") {
                            let data = "error".as_bytes();
                            write_logs([data, b" -> ",out.stdout.as_slice()].concat().as_slice()).await;
                            break;
                        }
                    }
                });
                tasks.push(task);
                
            };
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
        if check_interval.is_some() {
            if check_interval.unwrap() {
                let (_, interval) = update.date_interval().expect("check_interval in main error");
                let log_interval = ["update_interval_days=", interval.as_str(), "\n"].concat();
                update.clear_log_file().await;
                write_logs(update.time_now().as_bytes()).await;
                write_logs(log_interval.as_bytes()).await;
                let _ = update.tasks().await;
            } else { 
                return;
            }
        } else {
            // local git updates when we run file
            let none_interval = "update_interval_days=0\n".as_bytes();
            update.clear_log_file().await;
            write_logs(update.time_now().as_bytes()).await;
            write_logs(none_interval).await;
            let _ = update.tasks().await;
        }
    }
    
}

#[cfg(test)]
mod tests {
    use std::time::Duration;
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
