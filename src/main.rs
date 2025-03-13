use anyhow::Result;
use eframe::{
    egui::{
        Align, CentralPanel, Color32, Context, FontData, FontFamily, Layout, ScrollArea, TextEdit,
        TextStyle,
    },
    epaint::text::{FontInsert, FontPriority, InsertFontFamily},
    App, Frame, NativeOptions,
};
use std::{
    env,
    fs::File,
    io::{self, Read},
    mem::take,
    path::PathBuf,
    sync::mpsc::{channel, Receiver, Sender},
    thread::{spawn, JoinHandle},
};

use ffd::{scan_drivers, FullPath, Index, Volume};

fn main() -> eframe::Result {
    eframe::run_native(
        "FastFind",
        NativeOptions::default(),
        Box::new(|cc| {
            configure_font(&cc.egui_ctx).unwrap();

            Ok(Box::<FastFind>::default())
        }),
    )
}

fn configure_font(ctx: &Context) -> io::Result<()> {
    let mut path: PathBuf = env::var("SystemRoot")
        .unwrap_or(r"C:\Windows".to_string())
        .into();
    path.push(r"Fonts\msyh.ttc");
    let mut buf = Vec::new();
    File::open(path)?.read_to_end(&mut buf)?;
    ctx.add_font(FontInsert::new(
        "微软雅黑",
        FontData::from_owned(buf),
        vec![InsertFontFamily {
            family: FontFamily::Proportional,
            priority: FontPriority::Highest,
        }],
    ));

    Ok(())
}

#[derive(Default)]
struct FastFind {
    input: String,
    index_state: IndexState,
}

impl FastFind {
    fn find(&mut self, sub: String) {
        if let IndexState::Ready {
            sender,
            receiver,
            paths,
        } = &mut self.index_state
        {
            paths.clear();
            let (tx, rx) = channel();
            *receiver = rx;
            sender.send((sub, tx)).unwrap();
        }
    }

    fn sync(&mut self) {
        match &mut self.index_state {
            IndexState::Indxing(handles) => {
                if handles.iter().all(|h| h.is_finished()) {
                    let mut drvs: Vec<_> = take(handles)
                        .into_iter()
                        .map(|h| h.join().unwrap().unwrap())
                        .collect();

                    let (find_tx, find_rx) = channel();
                    let (res_tx, res_rx) = channel();
                    find_tx.send((String::new(), res_tx)).unwrap();
                    spawn(move || loop {
                        let (sub, res_tx) = find_rx.recv().unwrap();
                        // 空字符串不做搜索
                        if sub.is_empty() {
                            continue;
                        }

                        'outer: for (vol, idx) in &mut drvs {
                            idx.sync(vol).unwrap();
                            for path in idx.find_iter(&sub) {
                                if res_tx.send(path).is_err() {
                                    break 'outer;
                                }
                            }
                        }
                    });

                    self.index_state = IndexState::Ready {
                        sender: find_tx,
                        receiver: res_rx,
                        paths: Vec::new(),
                    };
                }
            }
            IndexState::Ready {
                sender: _,
                receiver,
                paths,
            } => {
                // 一次性接收太多会导致卡死
                paths.extend(receiver.try_iter().take(10));
            }
        }
    }
}

impl App for FastFind {
    fn update(&mut self, ctx: &Context, _: &mut Frame) {
        CentralPanel::default().show(ctx, |ui| {
            let text_edit = TextEdit::singleline(&mut self.input).hint_text("输入关键字");
            if ui
                .horizontal(|ui| ui.add_sized(ui.available_size(), text_edit))
                .inner
                .changed()
            {
                self.find(self.input.clone());
            }

            ui.separator();

            self.sync();
            match &self.index_state {
                IndexState::Indxing(_) => {
                    ui.label("索引中...");
                }
                IndexState::Ready {
                    sender: _,
                    receiver: _,
                    paths,
                } => {
                    let height = ui.text_style_height(&TextStyle::Body);
                    let total_rows = paths.len();
                    ScrollArea::vertical().show_rows(ui, height, total_rows, |ui, range| {
                        for path in &paths[range] {
                            let desired_size =
                                (ui.available_width(), ui.style().spacing.interact_size.y).into();
                            let layout = Layout::right_to_left(Align::Max);
                            ui.allocate_ui_with_layout(desired_size, layout, |ui| {
                                if ui.button("文件夹").clicked() {
                                    opener::reveal(path).unwrap();
                                };

                                if ui.button("打开").clicked() {
                                    opener::open(path.as_ref()).unwrap();
                                }

                                let layout =
                                    Layout::left_to_right(Align::Center).with_main_wrap(true);
                                ui.with_layout(layout, |ui| {
                                    let (prefix, sub, suffix) = path.split();
                                    ui.spacing_mut().item_spacing.x = 0.0;
                                    ui.label(prefix);
                                    ui.colored_label(Color32::RED, sub);
                                    ui.label(suffix);
                                });
                            });
                            ui.separator();
                        }
                    });
                }
            }
        });
    }
}

enum IndexState {
    Indxing(Vec<JoinHandle<Result<(Volume, Index)>>>),
    Ready {
        sender: Sender<(String, Sender<FullPath>)>,
        receiver: Receiver<FullPath>,
        paths: Vec<FullPath>,
    },
}

impl Default for IndexState {
    fn default() -> Self {
        let drvs = scan_drivers();
        let mut handles = Vec::with_capacity(drvs.len());
        for drv in drvs {
            let handle = spawn(move || {
                let vol = Volume::open(drv)?;
                let idx = Index::try_from_volume(&vol)?;
                Ok((vol, idx))
            });
            handles.push(handle);
        }

        Self::Indxing(handles)
    }
}
