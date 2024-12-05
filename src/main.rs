mod index;
mod ntfs;
mod style;

use clap::Parser;
use index::Index;
use memchr::memmem::{Finder, FinderRev};
use ntfs::Volume;
use nu_ansi_term::Color;
use std::io::{stdin, stdout, Write};
use style::Styled;

#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long, help = "一次性查询")]
    input: Option<String>,

    #[arg(long, help = "不使用彩色输出")]
    nocolor: bool,
}

fn main() {
    let mut args = Args::parse();
    // 忽略大小写
    args.input = args.input.map(|s| s.to_ascii_lowercase());

    let mut indices = Vec::new();
    // 根据输入判断是否为一次性查找
    let finder = args.input.as_ref().map(|input| Finder::new(input));
    let mut res = Vec::new();
    for name in Volume::names() {
        let volume = Volume::new(&name).unwrap();
        let mut index = Index::with_capacity(100000);
        let mut frns = Vec::new();
        for record in volume.iter_usn_record(4 * 1024 * 1024) {
            if let Some(finder) = &finder {
                if finder.find(record.filename.as_bytes()).is_some() {
                    frns.push(record.frn);
                }
            }
            index.set(record);
        }

        indices.push((name, index));
        res.push(frns);
    }

    if !args.nocolor {
        // Note for Windows 10 users: On Windows 10,
        // the application must enable ANSI support first:
        nu_ansi_term::enable_ansi_support().unwrap();
    }
    let style = Color::LightRed.bold();

    // 一次性查询，提前返回
    if let Some(finder) = finder {
        let rfinder = match args.nocolor {
            false => Some(FinderRev::new(finder.needle())),
            true => None,
        };
        let mut lock = stdout().lock();
        for (frns, (volume, index)) in res.into_iter().zip(indices) {
            for frn in frns {
                let name = index.full_name(frn);
                if let Some(rfinder) = &rfinder {
                    let styled = Styled::new(&style, &name, rfinder);
                    writeln!(lock, "{}{}", volume, styled).unwrap();
                } else {
                    writeln!(lock, "{}{}", volume, name).unwrap();
                }
            }
        }
        return;
    }

    // 进入持久化查询
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

        let finder = Finder::new(buf.trim());
        let rfinder = match args.nocolor {
            false => Some(FinderRev::new(finder.needle())),
            true => None,
        };
        let mut lock = stdout.lock();
        for (volume, index) in &indices {
            for (&frn, (_, name)) in index {
                if finder.find(name.to_ascii_lowercase().as_bytes()).is_some() {
                    let name = index.full_name(frn);
                    if let Some(rfinder) = &rfinder {
                        let styled = Styled::new(&style, &name, rfinder);
                        writeln!(lock, "{}{}", volume, styled).unwrap();
                    } else {
                        writeln!(lock, "{}{}", volume, name).unwrap();
                    }
                }
            }
        }
    }
}
