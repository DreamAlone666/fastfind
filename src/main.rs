use anyhow::{Context, Result};
use clap::Parser;
use env_logger::Env;
use log::error;
use nu_ansi_term::{Color, Style};
use std::{
    io::{stdin, stdout, Write},
    thread::{spawn, JoinHandle},
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

    let mut handles = Some(Vec::new());
    for drv in args.driver.unwrap_or_else(scan_drivers) {
        let handle: JoinHandle<Result<(Volume, Index)>> = spawn(move || {
            let vol = Volume::open(drv.clone()).with_context(|| format!("打开{drv:?}失败"))?;
            let idx = Index::try_from_volume(&vol).with_context(|| format!("索引{drv:?}失败"))?;
            Ok((vol, idx))
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
    let mut drivers = Vec::with_capacity(handles.as_ref().unwrap().len());
    loop {
        write!(stdout, "{}", prompt_style.paint(prompt)).unwrap();
        stdout.flush().unwrap();

        buf.clear();
        stdin.read_line(&mut buf).unwrap();

        let input = buf.trim();
        if input.is_empty() {
            continue;
        }

        // 等待索引完成
        if let Some(handles) = handles.take() {
            for handle in handles {
                match handle.join().unwrap() {
                    Ok(drv) => drivers.push(drv),
                    Err(e) => error!("{e:#}"),
                }
            }
        }

        for (vol, idx) in &mut drivers {
            if let Err(e) = idx.sync(vol) {
                error!("Index({:?}) 同步失败：{e}", idx.driver());
            }
            let mut stdout = stdout.lock();
            for mut path in idx.find_iter(input) {
                path.style(&style);
                writeln!(stdout, "{}", path).unwrap();
            }
        }
    }
}
