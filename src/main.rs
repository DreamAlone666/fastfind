mod index;
mod ntfs;

use clap::Parser;
use env_logger::Env;
use log::{debug, error};
use nu_ansi_term::{Color, Style};
use std::io::{stdin, stdout, Write};

use index::Index;
use ntfs::{scan_drivers, Volume};

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

    let mut volumes = Vec::new();
    let mut indices = Vec::new();
    for driver in args.driver.unwrap_or_else(scan_drivers) {
        let volume = match Volume::open(driver.clone()) {
            Ok(vol) => {
                debug!("Volume({:?})", vol.driver());
                vol
            }
            Err(e) => {
                error!("Volume({driver:?}): {e}");
                continue;
            }
        };
        let index = match Index::try_from_volume(&volume) {
            Ok(idx) => {
                debug!("Index({:?})", idx.driver());
                idx
            }
            Err(e) => {
                error!("Index({:?}): {e}", volume.driver());
                continue;
            }
        };
        volumes.push(volume);
        indices.push(index);
    }

    let style = if args.nocolor {
        Style::new()
    } else {
        // Note for Windows 10 users: On Windows 10,
        // the application must enable ANSI support first:
        nu_ansi_term::enable_ansi_support().unwrap();
        Color::LightRed.bold()
    };

    // 进入持久化查找
    let stdin = stdin();
    let mut stdout = stdout();
    let mut buf = String::new();
    let prompt = "[ffd]> ";
    let prompt_style = if args.nocolor {
        Style::new()
    } else {
        Color::LightGreen.bold()
    };
    loop {
        write!(stdout, "{}", prompt_style.paint(prompt)).unwrap();
        stdout.flush().unwrap();

        buf.clear();
        stdin.read_line(&mut buf).unwrap();

        for (index, volume) in indices.iter_mut().zip(&volumes) {
            if let Err(e) = index.sync(volume) {
                error!("Index({:?}) 同步失败：{e}", index.driver());
            }
            let mut lock = stdout.lock();
            for mut path in index.find_iter(buf.trim()) {
                path.style(&style);
                writeln!(lock, "{}", path).unwrap();
            }
        }
    }
}
