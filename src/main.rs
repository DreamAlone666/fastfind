mod index;
mod ntfs;
mod style;

use clap::Parser;
use env_logger::Env;
use log::{debug, error, info};
use memchr::memmem::FinderRev;
use nu_ansi_term::Color;
use std::io::{stdin, stdout, Write};

use index::Index;
use ntfs::{scan_drivers, Volume};
use style::Styled;

#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long, help = "一次性查询")]
    input: Option<String>,

    #[arg(long, help = "不使用彩色输出")]
    nocolor: bool,

    #[arg(long, help = "要搜索的盘")]
    volume: Option<Vec<String>>,

    #[arg(long, help = "默认日志等级设为debug")]
    verbose: bool,
}

fn main() {
    let mut args = Args::parse();
    // 忽略大小写
    args.input = args.input.map(|s| s.to_ascii_lowercase());

    if args.verbose {
        env_logger::Builder::from_env(Env::default().default_filter_or("debug")).init();
    } else {
        env_logger::init();
    }

    let mut indices = Vec::new();
    // 根据输入判断是否为一次性查找
    let finder = args.input.as_ref().map(|input| FinderRev::new(input));
    let mut res = Vec::new();
    for driver in args.volume.unwrap_or_else(scan_drivers) {
        let volume = match Volume::from_driver(driver.clone()) {
            Ok(vol) => {
                debug!("Volume({:?})", vol.driver());
                vol
            }
            Err(e) => {
                error!("Volume({driver:?}): {e}");
                continue;
            }
        };

        let mut index = Index::with_capacity(driver, 100000);
        let mut frns = Vec::new();
        let mut count = 0; // 记录遍历的日志数量
        for record in volume.iter_usn_record::<4096>() {
            let record = match record {
                Ok(r) => r,
                Err(e) => {
                    error!("IterUsnRecord({:?}): {e}", volume.driver());
                    break;
                }
            };

            if let Some(finder) = &finder {
                if finder.rfind(record.filename.as_bytes()).is_some() {
                    frns.push(record.frn);
                }
            }
            index.insert(record);
            count += 1;
        }

        info!("索引{}盘USN日志{}条", index.letter(), count);
        indices.push(index);
        res.push(frns);
    }

    if !args.nocolor {
        // Note for Windows 10 users: On Windows 10,
        // the application must enable ANSI support first:
        nu_ansi_term::enable_ansi_support().unwrap();
    }
    let style = Color::LightRed.bold();

    // 一次性查找，提前返回
    if let Some(finder) = finder {
        let mut lock = stdout().lock();
        for (frns, index) in res.into_iter().zip(indices) {
            for frn in frns {
                let name = index.get_path(frn).unwrap();
                if args.nocolor {
                    writeln!(lock, "{}", name).unwrap();
                } else {
                    let styled = Styled::new(&style, &name, &finder);
                    writeln!(lock, "{}", styled).unwrap();
                }
            }
        }
        return;
    }

    // 进入持久化查找
    let stdin = stdin();
    let mut stdout = stdout();
    let mut buf = String::new();
    loop {
        let prompt = "[ffd]> ";
        match args.nocolor {
            true => print!("{}", prompt),
            false => print!("{}", Color::LightGreen.bold().paint(prompt)),
        }
        stdout.flush().unwrap();

        buf.clear();
        stdin.read_line(&mut buf).unwrap();
        buf.make_ascii_lowercase();

        let finder = FinderRev::new(buf.trim());
        let mut lock = stdout.lock();
        for index in &indices {
            for (&frn, (_, name)) in index {
                if finder.rfind(name.to_ascii_lowercase().as_bytes()).is_some() {
                    let name = index.get_path(frn).unwrap();
                    if args.nocolor {
                        writeln!(lock, "{}", name).unwrap();
                    } else {
                        let styled = Styled::new(&style, &name, &finder);
                        writeln!(lock, "{}", styled).unwrap();
                    }
                }
            }
            info!("查找{}盘索引{}条", index.letter(), index.len());
        }
    }
}
