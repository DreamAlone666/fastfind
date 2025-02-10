use clap::Parser;
use env_logger::Env;
use log::{debug, error};
use nu_ansi_term::{Color, Style};
use std::{
    io::{stdin, stdout, Write},
    sync::{Arc, Mutex},
    thread::spawn,
};

use ffd::{scan_drivers, Index, Volume};

#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long, help = "不使用彩色输出")]
    nocolor: bool,

    #[arg(long, help = "要搜索的盘")]
    driver: Option<Vec<String>>,

    #[arg(long, help = "默认日志等级设为debug")]
    verbose: bool,
}

fn main() {
    let args = Args::parse();

    if args.verbose {
        env_logger::Builder::from_env(Env::default().default_filter_or("debug")).init();
    } else {
        env_logger::init();
    }

    let drivers = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Some(Vec::new());
    for driver in args.driver.unwrap_or_else(scan_drivers) {
        let drivers = Arc::clone(&drivers);
        let handle = spawn(move || {
            let volume = match Volume::open(driver.clone()) {
                Ok(vol) => {
                    debug!("Volume({:?})", vol.driver());
                    vol
                }
                Err(e) => {
                    error!("Volume({driver:?}): {e}");
                    return;
                }
            };
            let index: Index = match Index::try_from_volume(&volume) {
                Ok(idx) => {
                    debug!("Index({:?})", idx.driver());
                    idx
                }
                Err(e) => {
                    error!("Index({:?}): {e}", volume.driver());
                    return;
                }
            };
            drivers.lock().unwrap().push((volume, index));
        });
        handles.as_mut().unwrap().push(handle);
    }

    let (style, prompt_style) = if args.nocolor {
        (Style::new(), Style::new())
    } else {
        // Note for Windows 10 users: On Windows 10,
        // the application must enable ANSI support first:
        nu_ansi_term::enable_ansi_support().unwrap();
        (Color::LightRed.bold(), Color::LightGreen.bold())
    };

    // 进入持久化查找
    let stdin = stdin();
    let mut stdout = stdout();
    let mut buf = String::new();
    let prompt = "[ffd]> ";
    loop {
        write!(stdout, "{}", prompt_style.paint(prompt)).unwrap();
        stdout.flush().unwrap();

        buf.clear();
        stdin.read_line(&mut buf).unwrap();

        // 等待索引完成
        if let Some(handles) = handles.take() {
            for handle in handles {
                handle.join().unwrap();
            }
        }

        let mut drivers = drivers.lock().unwrap();
        for (vol, idx) in drivers.iter_mut() {
            if let Err(e) = idx.sync(vol) {
                error!("Index({:?}) 同步失败：{e}", idx.driver());
            }
            let mut stdout = stdout.lock();
            for mut path in idx.find_iter(buf.trim()) {
                path.style(&style);
                writeln!(stdout, "{}", path).unwrap();
            }
        }
    }
}
