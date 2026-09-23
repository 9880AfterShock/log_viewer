use std::{
    collections::{HashMap, VecDeque},
    env,
    fs::{self, File},
    io::Write,
    path::PathBuf,
    sync::{Arc, atomic::AtomicU64},
    thread::{self, JoinHandle},
};

use egui::{Button, Color32, Pos2, ProgressBar, Stroke, scroll_area};
use egui_async::{Bind, egui::AsyncButton};
use egui_plot::{HoverPosition, Line, Plot, PlotPoints, VLine};
use elegance::Theme;
use rfd::AsyncFileDialog;
use tokio::sync::RwLock;

use crate::log_api::{FileSpecifier, LogGrabber};

#[derive(PartialEq, Eq)]
pub enum OnDownloadAction {
    Load,
    Save,
}

#[derive(Clone, Debug)]
enum ObjectType {
    Robot,
    AprilTag,
    Marker,
}

impl TryFrom<&str> for ObjectType {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "ROBOT" => Ok(Self::Robot),
            "APRIL_TAG" => Ok(Self::AprilTag),
            "MARKER" => Ok(Self::Marker),
            _ => Err("failed to find a valid object type".to_string()),
        }
    }
}

impl TryFrom<String> for ObjectType {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.as_str().try_into()
    }
}

#[derive(Clone, Debug)]
struct FieldObject {
    name: String,
    object_type: ObjectType,
    color: i32,
    xpos: f64,
    ypos: f64,
    rot: f64,
}

impl TryFrom<&str> for FieldObject {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let mut split = value.split(",");
        let name = split
            .next()
            .map_or(Err("no next token".to_string()), |v| Ok(v))?
            .to_string();
        let object_type = ObjectType::try_from(
            split
                .next()
                .map_or(Err("no next token".to_string()), |v| Ok(v))?,
        )?;
        let color = split
            .next()
            .map_or(Err("no next token".to_string()), |v| Ok(v))?
            .parse::<i32>()
            .map_err(|v| v.to_string())?;
        let xpos = split
            .next()
            .map_or(Err("no next token".to_string()), |v| Ok(v))?
            .parse::<f64>()
            .map_err(|v| v.to_string())?;
        let ypos = split
            .next()
            .map_or(Err("no next token".to_string()), |v| Ok(v))?
            .parse::<f64>()
            .map_err(|v| v.to_string())?;
        let rot = split
            .next()
            .map_or(Err("no next token".to_string()), |v| Ok(v))?
            .parse::<f64>()
            .map_err(|v| v.to_string())?;
        Ok(Self {
            name,
            object_type,
            color,
            xpos,
            ypos,
            rot,
        })
    }
}

impl TryFrom<String> for FieldObject {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.as_str().try_into()
    }
}

#[derive(Clone, Debug)]
struct FieldState {
    field_objects: Vec<FieldObject>,
}

impl From<&str> for FieldState {
    fn from(value: &str) -> Self {
        Self {
            field_objects: value
                .split("|")
                .map(|v| v.try_into())
                .filter_map(|v| v.ok())
                .collect(),
        }
    }
}

impl From<String> for FieldState {
    fn from(value: String) -> Self {
        value.as_str().into()
    }
}

#[derive(Clone)]
pub enum LogEntry {
    Number {
        time: f64,
        name: String,
        data: f64,
    },
    CycleNumber {
        time: f64,
        name: String,
        data: u64,
    },
    String {
        time: f64,
        name: String,
        data: String,
    },
    Color {
        time: f64,
        name: String,
        data: i32,
    },
    Bool {
        time: f64,
        name: String,
        data: bool,
    },
    FieldState {
        time: f64,
        name: String,
        data: FieldState,
    },
    Unkown {
        time: f64,
        data_type: String,
        name: String,
        data: String,
    },
}

impl ToString for LogEntry {
    fn to_string(&self) -> String {
        match self {
            LogEntry::Number { time, name, data } => format!("[{time}] {name}: {data}"),
            LogEntry::CycleNumber { time, name, data } => format!("[{time}] {name}: {data}"),
            LogEntry::String { time, name, data } => format!("[{time}] {name}: {data}"),
            LogEntry::Color { time, name, data } => format!("[{time}] {name}: {data}"),
            LogEntry::Bool { time, name, data } => format!("[{time}] {name}: {data}"),
            LogEntry::FieldState { time, name, data } => format!("[{time}] {name}: {data:?}"),
            LogEntry::Unkown {
                time,
                data_type,
                name,
                data,
            } => format!("[{time}] {name}: {data}"),
        }
    }
}

impl LogEntry {
    fn get_time(&self) -> f64 {
        *match self {
            LogEntry::Number { time, name, data } => time,
            LogEntry::CycleNumber { time, name, data } => time,
            LogEntry::String { time, name, data } => time,
            LogEntry::Color { time, name, data } => time,
            LogEntry::Bool { time, name, data } => time,
            LogEntry::FieldState { time, name, data } => time,
            LogEntry::Unkown {
                time,
                data_type,
                name,
                data,
            } => time,
        }
    }
}

#[derive(PartialEq, Eq)]
enum Tab {
    LogLoader,
    EventTimeline,
    GraphViewer,
    FieldViewer,
}

pub struct LogViewer {
    loaded_log: Option<(Vec<LogEntry>, HashMap<(String, String), Vec<LogEntry>>)>,
    log_grabber: Arc<RwLock<LogGrabber>>,
    log_list: Option<Vec<FileSpecifier>>,
    download_bind: Bind<String, ()>,
    save_bind: Bind<(String, String), ()>,
    list_bind: Bind<Vec<FileSpecifier>, ()>,
    save_picker_bind: Bind<(), ()>,
    delete_bind: Bind<bool, ()>,
    upload_file_bind: Bind<String,()>,
    enabled_graphs: HashMap<String, bool>,
    tab: Tab,
    graph_picker_expanded: bool,
}

impl eframe::App for LogViewer {

    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if let Some(data) = self.upload_file_bind.take_ok() {
            self.load_log(data);
        }
        if let Some(data) = self.download_bind.take_ok() {
            self.load_log(data);
        }
        if let Some((_, _)) = self.save_bind.take_ok() {}
        if let Some(logs) = self.list_bind.take_ok() {
            self.log_list = Some(logs);
        }
        if let Some(succ) = self.delete_bind.take_ok() && succ {
            self.log_list = None;
        }
        ctx.plugin_or_default::<egui_async::EguiAsyncPlugin>();
    }
    
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        Theme::charcoal().install(ui.ctx());
        egui::Panel::top("Tab Bar").show(ui, |ui| {
            ui.horizontal(|ui| {
                let mut load_logs = Button::new("Load Logs");
                let mut event_timeline = Button::new("Event Timeline");
                let mut graph_viewer = Button::new("Graph Viewer");
                let mut field_viewer = Button::new("Field Viewer");
                load_logs = if Tab::LogLoader == self.tab {
                    load_logs.selected(true)
                } else {
                    load_logs
                };
                event_timeline = if Tab::EventTimeline == self.tab {
                    event_timeline.selected(true)
                } else {
                    event_timeline
                };
                graph_viewer = if Tab::GraphViewer == self.tab {
                    graph_viewer.selected(true)
                } else {
                    graph_viewer
                };
                field_viewer = if Tab::FieldViewer == self.tab {
                    field_viewer.selected(true)
                } else {
                    field_viewer
                };

                if ui.add(load_logs).clicked() {
                    self.tab = Tab::LogLoader;
                }

                if ui.add(event_timeline).clicked() {
                    self.tab = Tab::EventTimeline;
                }

                if ui.add(graph_viewer).clicked() {
                    self.tab = Tab::GraphViewer;
                }

                if ui.add(field_viewer).clicked() {
                    self.tab = Tab::FieldViewer;
                }
            });
        });
        egui::CentralPanel::default().show(ui, |ui| match self.tab {
            Tab::LogLoader => self.log_picker(ui, frame),
            Tab::EventTimeline => self.event_viewer(ui, frame),
            Tab::GraphViewer => self.graph_viewer(ui, frame),
            Tab::FieldViewer => self.field_viewer(ui, frame),
        });
    }
}

fn get_doc_path() -> PathBuf {
    return env::home_dir()
        .map(|v| v.join("Documents"))
        .unwrap_or("./".into());
}

impl LogViewer {
    pub async fn save_log(file_name: String, contents: String) {
        let mut picker = AsyncFileDialog::new().set_title("Save Log").add_filter("Log", &["log"]).set_file_name(file_name);
        #[cfg(not(any(target_arch = "wasm64",target_arch = "wasm32")))] 
        {
            picker = picker.set_directory(get_doc_path());
        }
        if let Some(file) = picker.save_file().await {
            file.write(contents.as_bytes()).await.expect("failed to write");
        }
        // let path = get_doc_path().join(file_name);
        // if fs::exists(&path).unwrap_or(false) {
        //     fs::remove_file(&path).expect("failed to delete file")
        // }
        // if let Ok(mut file) = File::create_new(&path) {
        //     file.write_all(contents.as_bytes())
        //         .expect("failed to write");
        // }
    }

    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            loaded_log: None,
            log_grabber: Arc::new(RwLock::new(LogGrabber::new())),
            tab: Tab::LogLoader,
            enabled_graphs: HashMap::new(),
            graph_picker_expanded: true,
            upload_file_bind: Bind::new(true),
            download_bind: Bind::new(true),
            save_bind: Bind::new(true),
            list_bind: Bind::new(true),
            delete_bind: Bind::new(true),
            log_list: None,
            save_picker_bind: Bind::new(true),
        }
    }

    fn load_log(&mut self, log: String) {
        let mut log_vec: Vec<LogEntry> = Vec::new();
        let mut sorted_entries: HashMap<(String, String), Vec<LogEntry>> = HashMap::new();
        for line in log.split("\n") {
            if let Some((prefix, data)) = line.split_once(" : ")
                && let Some((time_name, data_type)) = prefix.split_once(" | ")
                && let Some((time_str, name)) = time_name.replacen("[", "", 1).split_once("] ")
                && let Ok(time) = time_str.parse::<f64>()
            {
                let entry = match data_type {
                    "String" => LogEntry::String {
                        time,
                        name: name.to_string(),
                        data: data.to_string(),
                    },
                    "Number" => LogEntry::Number {
                        time,
                        name: name.to_string(),
                        data: data.parse().unwrap_or_default(),
                    },
                    "Bool" => LogEntry::Bool {
                        time,
                        name: name.to_string(),
                        data: data.parse().unwrap_or(false),
                    },
                    "CycleNumber" => LogEntry::CycleNumber {
                        time,
                        name: name.to_string(),
                        data: data.parse().expect("err in cycle number parsing"),
                    },
                    "Color" => LogEntry::Color {
                        time,
                        name: name.to_string(),
                        data: data.parse().unwrap_or_default(),
                    },
                    "FieldState" => LogEntry::FieldState {
                        time,
                        name: name.to_string(),
                        data: data.into(),
                    },
                    _ => LogEntry::Unkown {
                        time,
                        data_type: data_type.to_string(),
                        name: name.to_string(),
                        data: data.to_string(),
                    },
                };
                if let Some(list) =
                    sorted_entries.get_mut(&(name.to_string(), data_type.to_string()))
                {
                    list.push(entry.clone());
                } else {
                    sorted_entries.insert(
                        (name.to_string(), data_type.to_string()),
                        vec![entry.clone()],
                    );
                }
                log_vec.push(entry);
            }
        }
        self.loaded_log = Some((log_vec, sorted_entries));
    }

    fn field_viewer(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            let next = ui.next_widget_position();
            let size = ui.available_size();
            ui.painter().line_segment(
                [next, next+size],
                Stroke::new(5.0, Color32::RED),
            )
        });
    }

    fn graph_viewer(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if let Some((log, sorted_logs)) = self.loaded_log.as_ref() {
            let mut lines: Vec<Line> = Vec::new();
            let mut vlines: Vec<VLine> = Vec::new();
            for (name, data_type) in sorted_logs.keys() {
                match data_type.as_str() {
                    "Number" => {
                        if !self.enabled_graphs.contains_key(name) {
                            self.enabled_graphs.insert(name.clone(), false);
                        }
                        if *self.enabled_graphs.get(name).unwrap_or(&false) {
                            let points: PlotPoints = sorted_logs
                                .get(&(name.clone(), data_type.clone()))
                                .expect("???")
                                .iter()
                                .map(|v| {
                                    #[cfg(not(any(target_arch = "wasm64",target_arch = "wasm32")))]
                                    if let LogEntry::Number { time, name, data } = v {
                                        [*time, *data]
                                    } else {
                                        [v.get_time(), 0.0]
                                    }
                                    #[cfg(any(target_arch = "wasm64",target_arch = "wasm32"))]
                                    if let LogEntry::Number { time, name, data } = v {
                                        [time.clone(), data.clone()]
                                    } else {
                                        [v.get_time(), 0.0]
                                    }
                                })
                                .collect();
                            let line = Line::new(name, points);
                            lines.push(line);
                        }
                    }
                    "CycleNumber" => {
                        if !self.enabled_graphs.contains_key(name) {
                            self.enabled_graphs.insert(name.clone(), false);
                        }
                        if *self.enabled_graphs.get(name).unwrap_or(&false) {
                            sorted_logs
                                .get(&(name.clone(), data_type.clone()))
                                .expect("???")
                                .iter()
                                .for_each(|v| {
                                    if let LogEntry::CycleNumber { time, name, data } = v {
                                        vlines.push(VLine::new(format!("Cylce #{}", data), *time));
                                    }
                                });
                        }
                    }
                    _ => {}
                }
            }
            egui::Panel::left("plot_picker")
                .drag_to_open(true)
                .show_collapsible(ui, &mut self.graph_picker_expanded, |ui| {
                    for line_plot in self.enabled_graphs.iter_mut() {
                        ui.horizontal(|ui| {
                            ui.checkbox(line_plot.1, line_plot.0);
                        });
                    }
                });
            egui::CentralPanel::default().show(ui, |ui| {
                ui.separator();
                Plot::new("main_plot")
                    .clamp_grid(true)
                    .show_grid([false, true])
                    .label_formatter(|pos| match pos {
                        HoverPosition::NearDataPoint {
                            plot_name,
                            position,
                            ..
                        } if !plot_name.is_empty() => {
                            Some(format!("{}: {}", plot_name, position.y))
                        }
                        _ => None,
                    })
                    .show(ui, |plot_ui| {
                        let bounds = plot_ui.plot_bounds();
                        let width = bounds.width();
                        let transparency =
                            (255. - (width - 1.0).clamp(0.0, 5.0) * (255. / 5.)) as u8;
                        if transparency > 0 {
                            for vline in &vlines {
                                plot_ui.vline(vline.clone().color(
                                    Color32::from_rgba_unmultiplied(100, 100, 100, transparency),
                                ));
                            }
                        }
                        for line in lines {
                            plot_ui.line(line);
                        }
                    });
            });
        }
    }

    fn event_viewer(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if let Some((log, _)) = self.loaded_log.as_ref() {
            let height = ui.text_style_height(&egui::TextStyle::Body);
            scroll_area::ScrollArea::vertical().show_rows(
                ui,
                height,
                log.len(),
                |ui, row_range| {
                    for index in row_range {
                        ui.label(&log[index].to_string());
                        ui.separator();
                    }
                },
            );
        }
    }

    fn log_picker(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if let Some(logs) = &self.log_list {
            for log in logs {
                ui.horizontal(|ui| {
                    ui.label(&log.name);
                    let load_log_name = log.name.clone();
                    let save_log_name = log.name.clone();
                    let delete_log_name = log.name.clone();
                    let log_grabber_load = self.log_grabber.clone();
                    let log_grabber_save = self.log_grabber.clone();
                    let log_grabber_delete = self.log_grabber.clone();
                    AsyncButton::new(&mut self.download_bind,"Load").show(ui, async move || {
                        log_grabber_load.read().await.read_log(load_log_name).await.map_err(|_| ())
                    });
                    AsyncButton::new(&mut self.save_bind,"Save").show(ui, async move || {
                        if let Ok((name,data)) = log_grabber_save.read().await.read_log(&save_log_name).await.map_err(|_| ()).map(|v| (save_log_name,v)) {
                            Self::save_log(name.clone(), data.clone()).await;
                            Ok((name,data))
                        } else {
                            Err(())
                        }
                    });
                    AsyncButton::new(&mut self.delete_bind,"Delete").show(ui, async move || {
                        log_grabber_delete.read().await.delete_log(delete_log_name).await.map_err(|_| ())
                    });
                });
            }
        } else {
            let log_grabber_list = self.log_grabber.clone();
            self.list_bind.request(async move {
                log_grabber_list.read().await.list_logs().await.map_err(|_| ())
            });
        }
        egui::Panel::bottom("Load File Panel").show(ui, |ui| {
            let mut picker = AsyncFileDialog::new().set_title("Load Log").add_filter("Log", &["log"]);
            #[cfg(not(any(target_arch = "wasm64",target_arch = "wasm32")))] 
            {
                picker = picker.set_directory(get_doc_path());
            }
            AsyncButton::new(&mut self.upload_file_bind, "Load From File").show(ui,async move|| {
                if let Some(handle) = picker.pick_file().await && let Ok(result) = String::from_utf8(handle.read().await) {
                    Ok(result)
                } else {
                    Err(())
                }
            });
        });
    }
}
