use std::env::current_dir;
use std::fs;
use std::fs::read_dir;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use tokio::process::Command;
use std::str::from_utf8;
use chrono::Local;



async fn write_logs(data: &[u8]) {
    let p = current_dir()
        .unwrap()
        .as_path()
        .join("update_repo.log");

    let _ = fs::File::options()
        //.write(true)
        .append(true)
        .open(p)
        .expect("error log writes")
        .write(data);
}

#[tokio::main]
async fn main() {
    
    let log_file = current_dir()
        .unwrap()
        .as_path()
        .join("update_repo.log");
    
    if !log_file.exists() {
        fs::File::create(&log_file)
            .expect("something wrong with creating log_file");
    } else { tokio::fs::File::options()
        .write(true)
        .truncate(true)
        .open(log_file.as_path())
        .await
        .expect("prob with clear log"); }
    
    // write data to log
    let time = chrono::Utc::now().with_timezone(&Local);
    let mut time = time.format("%Y-%m-%d %H:%M:%S").to_string();
    time.push('\n');
    let d = time.as_bytes();
    write_logs(d).await;
    
    // path for works with 
    //let path = current_dir().unwrap();
    let path = Path::new("/home/den/venv/STUDY/").to_path_buf();
    
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
    let mut tasks = Vec::with_capacity(nested_dirs.len());
    for i in nested_dirs {
        tasks.push(tokio::spawn(async move {
            let out = git_url(i.clone()).await;
            // check if our repository are exists
            if !out.status.success() {
                write_logs(out.stderr.as_slice()).await;
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
                                | l.contains("error"));
    
                match result {
                    Some(a) if a.contains("Уже актуально") => {
                        let data = result.unwrap().as_bytes();
                        write_logs([data, b" -> ",out.stdout.as_slice()].concat().as_slice()).await;
                    }
                    Some(a) if a.contains("Получение обьектов") => {
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