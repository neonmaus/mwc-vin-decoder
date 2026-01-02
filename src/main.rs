#![windows_subsystem = "windows"]

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;

use byteorder::LittleEndian;
use byteorder::ReadBytesExt;

/// Empty string constant
const EMPTY: &str = "";
/// Entry start byte in binary format
const HX_START_ENTRY: u8 = 0x7E;

// VINGen4 binary format type identifiers
const CONTAINER_TYPE_DICTIONARY: u8 = 0x52;
const VALUE_TYPE_STRING: u32 = 0xFDE9F1EE;
const VALUE_TYPE_INT32: u32 = 0xE2A80856;
const VALUE_TYPE_BOOL: u32 = 0xAD4D7C9C;

/// VIN field definition (key, display name, length)
#[derive(Debug)]
struct VinField {
    /// Field key for lookup
    key: &'static str,
    /// Human-readable name
    display: &'static str,
    /// Field length in VIN
    len: usize,
}

/// Tracks VIN data source
enum LastSource {
    None,
    File,
    Vin,
}

/// Ordered VIN field structure
const VIN_STRUCTURE: &[VinField] = &[
    VinField {
        key: "Country",
        display: "Country",
        len: 1,
    },
    VinField {
        key: "AssemblyPlant",
        display: "Assembly Plant",
        len: 1,
    },
    VinField {
        key: "Model",
        display: "Model",
        len: 1,
    },
    VinField {
        key: "Body",
        display: "Body",
        len: 1,
    },
    VinField {
        key: "Version",
        display: "Version",
        len: 1,
    },
    VinField {
        key: "Year",
        display: "Year",
        len: 1,
    },
    VinField {
        key: "Month",
        display: "Month",
        len: 1,
    },
    VinField {
        key: "Serial",
        display: "Serial",
        len: 5,
    },
    VinField {
        key: "Drive",
        display: "Drive",
        len: 1,
    },
    VinField {
        key: "Engine",
        display: "Engine",
        len: 2,
    },
    VinField {
        key: "Gearbox",
        display: "Gearbox",
        len: 1,
    },
    VinField {
        key: "AxleRatio",
        display: "Axle Ratio",
        len: 1,
    },
    VinField {
        key: "AxleLock",
        display: "Axle Lock",
        len: 1,
    },
    VinField {
        key: "ColorsBody",
        display: "Body Colour",
        len: 1,
    },
    VinField {
        key: "VinylRoof",
        display: "Vinyl Roof",
        len: 1,
    },
    VinField {
        key: "InteriorTrim",
        display: "Interior Trim",
        len: 1,
    },
    VinField {
        key: "Radio",
        display: "Radio",
        len: 1,
    },
    VinField {
        key: "InstrumentPanel",
        display: "Instrument Panel",
        len: 1,
    },
    VinField {
        key: "Windshield",
        display: "Windshield",
        len: 1,
    },
    VinField {
        key: "Seats",
        display: "Seats",
        len: 1,
    },
    VinField {
        key: "Suspension",
        display: "Suspension",
        len: 1,
    },
    VinField {
        key: "PowerBrakes",
        display: "Brakes",
        len: 1,
    },
    VinField {
        key: "Wheels",
        display: "Wheels",
        len: 1,
    },
    VinField {
        key: "WindowHeater",
        display: "Rear Window",
        len: 1,
    },
];

/// Parse VINGen4 header, returns (container_type, key_type, value_type, offset)
fn read_header(body: &[u8]) -> Option<(u8, u32, u32, usize)> {
    let mut offset = 1;
    let container_type = if body[0] != 0xFF { body[0] } else { 0x00 };
    offset += 1;
    let mut key_type = 0u32;
    if container_type == CONTAINER_TYPE_DICTIONARY {
        key_type = (&body[offset..offset + 4])
            .read_u32::<LittleEndian>()
            .ok()?;
        offset += 4;
    }
    let value_type = (&body[offset..offset + 4])
        .read_u32::<LittleEndian>()
        .ok()?;
    offset += 4;
    let prop_size = if key_type == 0 { 1 } else { 2 };
    offset += prop_size;
    Some((container_type, key_type, value_type, offset))
}

/// Parse value from binary (string, int32, bool, or hex)
fn parse_value(data: &[u8], offset: &mut usize, value_type: u32) -> Option<String> {
    match value_type {
        VALUE_TYPE_STRING => {
            let strlen = data.get(*offset).copied()? as usize;
            let s = std::str::from_utf8(&data[*offset + 1..*offset + 1 + strlen])
                .unwrap_or("")
                .to_string();
            *offset += 1 + strlen;
            Some(s)
        }
        VALUE_TYPE_INT32 => {
            if *offset + 4 > data.len() {
                return None;
            }
            let val = (&data[*offset..*offset + 4])
                .read_i32::<LittleEndian>()
                .ok()?;
            *offset += 4;
            Some(val.to_string())
        }
        VALUE_TYPE_BOOL => {
            let b = data.get(*offset).copied()? != 0;
            *offset += 1;
            Some(if b { "true" } else { "false" }.to_string())
        }
        _ => {
            if *offset + 4 > data.len() {
                return None;
            }
            let hex = data[*offset..*offset + 4]
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<String>();
            *offset += 4;
            Some(hex)
        }
    }
}

/// Parse binary dictionary into key-value pairs
fn parse_dictionary_vec(data: &[u8], key_type: u32, value_type: u32) -> Vec<(String, String)> {
    if data.len() < 4 {
        return vec![];
    }
    let count = (&data[0..4]).read_u32::<LittleEndian>().unwrap_or(0) as usize;
    let mut offset = 4;
    (0..count)
        .map(|_| {
            let key = parse_value(data, &mut offset, key_type).unwrap_or_default();
            let val = parse_value(data, &mut offset, value_type).unwrap_or_default();
            (key, val)
        })
        .collect()
}

/// Read VINGen4 section from carparts.txt
fn parse_vingen4_file(path: &str) -> Option<Vec<(String, String)>> {
    let mut file = File::open(path).ok()?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).ok()?;
    let mut i = 0;
    while i < buffer.len() {
        if buffer[i] != HX_START_ENTRY {
            i += 1;
            continue;
        }
        if i + 1 >= buffer.len() {
            break;
        }
        let tag_size = buffer[i + 1] as usize;
        if i + 2 + tag_size + 4 > buffer.len() {
            break;
        }
        let tag = String::from_utf8_lossy(&buffer[i + 2..i + 2 + tag_size]);
        let body_len = u32::from_le_bytes([
            buffer[i + 2 + tag_size],
            buffer[i + 3 + tag_size],
            buffer[i + 4 + tag_size],
            buffer[i + 5 + tag_size],
        ]) as usize;
        let body_start = i + 2 + tag_size + 4;
        let body_end = body_start + body_len;
        if body_end > buffer.len() {
            break;
        }
        if tag == "VINGen4" {
            let body = &buffer[body_start..body_end];
            if let Some((ctype, ktype, vtype, offset)) = read_header(body) {
                if ctype == CONTAINER_TYPE_DICTIONARY {
                    let dict = parse_dictionary_vec(&body[offset..], ktype, vtype);
                    return Some(dict);
                }
            }
        }
        i = body_end;
    }
    None
}

/// VIN field decode tables
fn decode_map() -> HashMap<&'static str, HashMap<&'static str, &'static str>> {
    let mut map = HashMap::new();
    map.insert("Country", HashMap::from_iter([("U", "Corris Britain")]));
    map.insert(
        "AssemblyPlant",
        HashMap::from_iter([
            ("A", "Dagenham"),
            ("B", "Manchester"),
            ("C", "Saarlouis"),
            ("K", "Rheine"),
        ]),
    );
    map.insert("Model", HashMap::from_iter([("B", "Rivett")]));
    map.insert("Body", HashMap::from_iter([("B", "2D Pillared Sedan")]));
    map.insert(
        "Version",
        HashMap::from_iter([("D", "L"), ("E", "LX"), ("G", "SLX"), ("P", "GT")]),
    );
    map.insert(
        "Year",
        HashMap::from_iter([
            ("L", "1971"),
            ("M", "1972"),
            ("N", "1973"),
            ("P", "1974 (Facelift)"),
            ("R", "1975"),
            ("S", "1976"),
        ]),
    );
    map.insert(
        "Month",
        HashMap::from_iter([
            ("C", "01"),
            ("K", "02"),
            ("D", "03"),
            ("E", "04"),
            ("L", "05"),
            ("Y", "06"),
            ("S", "07"),
            ("T", "08"),
            ("J", "09"),
            ("U", "10"),
            ("M", "11"),
            ("P", "12"),
        ]),
    );
    map.insert("Drive", HashMap::from_iter([("1", "RWD")]));
    map.insert(
        "Engine",
        HashMap::from_iter([("NA", "Standard 2.0"), ("NE", "High Performance 2.0")]),
    );
    map.insert(
        "Gearbox",
        HashMap::from_iter([("7", "3-spd Automatic"), ("B", "4-spd Manual")]),
    );
    map.insert(
        "AxleRatio",
        HashMap::from_iter([
            ("S", "3.44"),
            ("B", "3.75"),
            ("C", "3.89"),
            ("N", "4.11"),
            ("E", "4.44"),
        ]),
    );
    map.insert(
        "AxleLock",
        HashMap::from_iter([("A", "Open"), ("B", "LSD")]),
    );
    map.insert(
        "ColorsBody",
        HashMap::from_iter([
            ("A", "Dark Grey"),
            ("B", "Nature White"),
            ("C", "Sand"),
            ("D", "Asphalt Grey"),
            ("E", "Blue"),
            ("F", "Sun Yellow"),
            ("G", "Dark Navy"),
            ("H", "Royal Red"),
            ("I", "Brown"),
            ("J", "Red"),
            ("K", "Electric Green"),
            ("L", "White Pearl"),
            ("M", "Spring Green"),
            ("R", "Purple"),
            ("T", "Yellow"),
            ("U", "Sky Blue"),
            ("V", "Orange"),
            ("X", "Navy Blue"),
            ("Y", "Special"),
        ]),
    );
    map.insert(
        "VinylRoof",
        HashMap::from_iter([
            ("-", "Paint"),
            ("A", "Black"),
            ("B", "White"),
            ("C", "Tan"),
            ("K", "Blue"),
            ("M", "Dark Brown"),
        ]),
    );
    map.insert(
        "InteriorTrim",
        HashMap::from_iter([
            ("N", "Red"),
            ("A", "Black"),
            ("K", "Tan"),
            ("F", "Blue"),
            ("Y", "Special"),
        ]),
    );
    map.insert(
        "Radio",
        HashMap::from_iter([("-", "Radio delete"), ("J", "Radio")]),
    );
    map.insert(
        "InstrumentPanel",
        HashMap::from_iter([("-", "Standard"), ("G", "Clock"), ("M", "Tachometer")]),
    );
    map.insert(
        "Windshield",
        HashMap::from_iter([("1", "Clear"), ("2", "Tinted"), ("F", "Sunstrip")]),
    );
    map.insert(
        "Seats",
        HashMap::from_iter([("8", "Standard"), ("B", "Bucket Style")]),
    );
    map.insert(
        "Suspension",
        HashMap::from_iter([
            ("A", "Standard"),
            ("B", "Standard + Stiffened"),
            ("4", "Lowered"),
            ("M", "Lowered + Stiffened"),
        ]),
    );
    map.insert(
        "PowerBrakes",
        HashMap::from_iter([("-", "Standard"), ("B", "Power Brakes")]),
    );
    map.insert(
        "Wheels",
        HashMap::from_iter([
            ("A", "13\" Steel"),
            ("B", "13\" Steel + hubcaps"),
            ("4", "14\" Sport"),
            ("M", "14\" Steel / 14\" Octo"),
        ]),
    );
    map.insert(
        "WindowHeater",
        HashMap::from_iter([
            ("-", "Standard"),
            ("B", "Heated"),
            ("M", "Standard + Window Grille"),
        ]),
    );
    map
}

/// Split VIN string into fields
fn parse_vin(vin: &str) -> HashMap<String, String> {
    let mut pos = 0;
    VIN_STRUCTURE
        .iter()
        .map(|field| {
            let end = pos + field.len;
            let val = vin.get(pos..end).unwrap_or("").to_string();
            pos = end;
            (field.key.to_string(), val)
        })
        .collect()
}

/// Get color for field code (for GUI swatches)
fn color_for_code_with_field(field: &str, code: &str) -> Option<egui::Color32> {
    match field {
        "ColorsBody" => match code {
            "A" => Some(egui::Color32::from_rgb(64, 64, 64)), // Dark Grey
            "B" => Some(egui::Color32::from_rgb(240, 240, 240)), // Nature White
            "C" => Some(egui::Color32::from_rgb(210, 180, 140)), // Sand
            "D" => Some(egui::Color32::from_rgb(80, 80, 80)), // Asphalt Grey
            "E" => Some(egui::Color32::from_rgb(0, 80, 200)), // Blue
            "F" => Some(egui::Color32::from_rgb(255, 220, 40)), // Sun Yellow
            "G" => Some(egui::Color32::from_rgb(10, 10, 60)), // Dark Navy
            "H" => Some(egui::Color32::from_rgb(180, 0, 0)),  // Royal Red
            "I" => Some(egui::Color32::from_rgb(120, 80, 40)), // Brown
            "J" => Some(egui::Color32::from_rgb(200, 0, 0)),  // Red
            "K" => Some(egui::Color32::from_rgb(0, 200, 80)), // Electric Green
            "L" => Some(egui::Color32::from_rgb(255, 255, 255)), // White Pearl
            "M" => Some(egui::Color32::from_rgb(120, 255, 120)), // Spring Green
            "R" => Some(egui::Color32::from_rgb(160, 0, 160)), // Purple
            "T" => Some(egui::Color32::from_rgb(255, 255, 0)), // Yellow
            "U" => Some(egui::Color32::from_rgb(120, 180, 255)), // Sky Blue
            "V" => Some(egui::Color32::from_rgb(255, 120, 0)), // Orange
            "X" => Some(egui::Color32::from_rgb(0, 0, 120)),  // Navy Blue
            "Y" => Some(egui::Color32::from_rgb(212, 175, 55)), // Special (gold)
            _ => None,
        },
        "VinylRoof" => match code {
            "-" => Some(egui::Color32::from_rgb(200, 200, 200)), // Paint
            "A" => Some(egui::Color32::from_rgb(20, 20, 20)),    // Black
            "B" => Some(egui::Color32::from_rgb(255, 255, 255)), // White
            "C" => Some(egui::Color32::from_rgb(210, 180, 140)), // Tan
            "K" => Some(egui::Color32::from_rgb(0, 80, 200)),    // Blue
            "M" => Some(egui::Color32::from_rgb(80, 40, 20)),    // Dark Brown
            _ => None,
        },
        "InteriorTrim" => match code {
            "N" => Some(egui::Color32::from_rgb(200, 0, 0)), // Red
            "A" => Some(egui::Color32::from_rgb(20, 20, 20)), // Black
            "K" => Some(egui::Color32::from_rgb(210, 180, 140)), // Tan
            "F" => Some(egui::Color32::from_rgb(0, 80, 200)), // Blue
            "Y" => Some(egui::Color32::from_rgb(212, 175, 55)), // Special (gold)
            _ => None,
        },
        _ => None,
    }
}

/// Render color swatch for color fields
fn render_color_swatch<'a>(
    ui: &mut egui::Ui,
    field_key: &str,
    val: &str,
    body_color_getter: impl FnOnce() -> Option<&'a str>,
) {
    if matches!(field_key, "ColorsBody" | "VinylRoof" | "InteriorTrim") {
        let mut color = color_for_code_with_field(field_key, val);
        // VinylRoof = Paint uses body color
        if field_key == "VinylRoof" && val == "-" {
            if let Some(body_val) = body_color_getter() {
                color = color_for_code_with_field("ColorsBody", body_val);
            }
        }
        if let Some(color) = color {
            let (rect, _resp) =
                ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 3.0, color);
        }
    }
}

/// Show info for special VIN combinations
fn show_info_labels(ui: &mut egui::Ui, v: &str, i: &str) {
    if v == "G" && i == "M" {
        ui.vertical_centered(|ui| {
            ui.label("SLX + Tachometer Package: Center console & Sport steering wheel ");
        });
    } else if v == "P" {
        ui.vertical_centered(|ui| {
            ui.label(
                "GT Equipment: Sport steering wheel, Special gear stick & Quick steering ratio",
            );
        });
    }
}

/// Render VIN decode table with given data source
fn render_vin_table<'a>(
    ui: &mut egui::Ui,
    decode_map: &HashMap<&'static str, HashMap<&'static str, &'static str>>,
    get_value: impl Fn(&str) -> &'a str,
) {
    ui.horizontal(|ui| {
        ui.add_space((ui.available_width() - 380.0) / 2.0);
        egui::Frame::new()
            .inner_margin(10.0)
            .outer_margin(5.0)
            .corner_radius(2.0)
            .fill(egui::Color32::from_rgb(45, 45, 47))
            .stroke(egui::Stroke::new(
                3.0,
                egui::Color32::from_rgb(100, 100, 105),
            ))
            .show(ui, |ui| {
                egui::Grid::new("vin_table")
                    .striped(true)
                    .spacing([10.0, 4.0])
                    .min_col_width(80.0)
                    .show(ui, |ui| {
                        ui.strong("Field");
                        ui.strong("Value");
                        ui.strong("Decoded");
                        ui.end_row();
                        for field in VIN_STRUCTURE {
                            let val = get_value(field.key);
                            let status = match decode_map.get(field.key).and_then(|m| m.get(val)) {
                                Some(d) => d,
                                None if val == "-" => "Standard / None",
                                None if field.key != "Serial" && !val.is_empty() => {
                                    "!! [UNKNOWN] !!"
                                }
                                _ => "",
                            };
                            ui.label(field.display);
                            ui.label(val);
                            ui.horizontal(|ui| {
                                ui.label(status);
                                render_color_swatch(ui, field.key, val, || {
                                    Some(get_value("ColorsBody"))
                                });
                            });
                            ui.end_row();
                        }
                    });
            });
    });

    ui.add_space(8.0);
    let v_val = get_value("Version");
    let i_val = get_value("InstrumentPanel");
    show_info_labels(ui, v_val, i_val);

    let complete_vin: String = VIN_STRUCTURE.iter().map(|f| get_value(f.key)).collect();
    ui.separator();
    ui.vertical_centered(|ui| {
        ui.monospace(format!("Complete VIN: {}", complete_vin));
    });
}

/// VIN Decoder application state
struct VinApp {
    vin_input: String,
    entries: Option<HashMap<String, String>>,
    vin_error: Option<String>,
    file_path: String,
    vingen4_entries: Option<Vec<(String, String)>>,
    last_source: LastSource,
    decode_map: HashMap<&'static str, HashMap<&'static str, &'static str>>,
    file_error: Option<String>,
}

impl VinApp {
    /// Get default carparts.txt path
    fn default_file_path() -> String {
        if let Ok(userprofile) = std::env::var("USERPROFILE") {
            format!(
                "{}\\AppData\\LocalLow\\Amistech\\My Winter Car\\carparts.txt",
                userprofile
            )
        } else {
            String::new()
        }
    }
}

/// Default values (including carparts.txt path)
impl Default for VinApp {
    fn default() -> Self {
        Self {
            vin_input: String::new(),
            entries: None,
            vin_error: None,
            file_path: VinApp::default_file_path(),
            vingen4_entries: None,
            last_source: LastSource::None,
            decode_map: decode_map(),
            file_error: None,
        }
    }
}

/// GUI update loop
impl eframe::App for VinApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1999 Werkstatt-Style in Dark Mode
        let mut style = (*ctx.style()).clone();

        // Dunkle 90er Werkstatt-Farben
        let bg_color = egui::Color32::from_rgb(30, 30, 32); // Dunkler Hintergrund
        let panel_color = egui::Color32::from_rgb(40, 40, 42); // Panel Hintergrund
        let border_color = egui::Color32::from_rgb(100, 100, 105); // Grauer Rahmen
        let werkstatt_orange = egui::Color32::from_rgb(200, 120, 40); // Werkstatt-Orange
        let metal_dark = egui::Color32::from_rgb(60, 60, 65); // Dunkles Metall

        style.visuals.panel_fill = bg_color;
        style.visuals.window_fill = bg_color;
        style.visuals.faint_bg_color = panel_color;
        style.visuals.extreme_bg_color = egui::Color32::from_rgb(50, 50, 52);
        style.visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(45, 45, 47);
        style.visuals.widgets.inactive.bg_fill = metal_dark;
        style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(70, 70, 75);
        style.visuals.widgets.active.bg_fill = werkstatt_orange;
        style.visuals.selection.bg_fill = werkstatt_orange;
        style.visuals.window_stroke = egui::Stroke::new(2.0, border_color);
        style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(2.0, border_color);

        style.text_styles = [
            (egui::TextStyle::Heading, egui::FontId::proportional(20.0)),
            (egui::TextStyle::Body, egui::FontId::proportional(14.0)),
            (egui::TextStyle::Monospace, egui::FontId::monospace(14.0)),
            (egui::TextStyle::Button, egui::FontId::proportional(14.0)),
            (egui::TextStyle::Small, egui::FontId::proportional(12.0)),
        ]
        .into();
        ctx.set_style(style);

        // Handle file drag-and-drop: accept a dropped `carparts.txt`file
        // and attempt to parse it as a VINGen4 file. We prefer the first dropped
        // file with a native path, otherwise fall back to the first bytes payload.
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if !dropped.is_empty() {
            for df in dropped.into_iter() {
                if let Some(path) = df.path {
                    let path_str = path.display().to_string();
                    self.file_path = path_str.clone();
                    if !std::path::Path::new(&self.file_path).exists() {
                        self.file_error = Some(format!("File not found: {}", self.file_path));
                        self.vingen4_entries = None;
                    } else {
                        match parse_vingen4_file(&self.file_path) {
                            Some(entries) => {
                                self.vingen4_entries = Some(entries);
                                self.file_error = None;
                                self.last_source = LastSource::File;
                            }
                            None => {
                                self.file_error = Some("No VIN data found in file".to_string());
                                self.vingen4_entries = None;
                            }
                        }
                    }
                    break;
                }

                // If there's no native path but bytes were dropped (e.g., from the web),
                // write them to a temp file and attempt to parse that.
                if let Some(bytes) = df.bytes.clone() {
                    use std::io::Write;
                    let tmp = std::env::temp_dir().join("dropped_carparts.txt");
                    if let Ok(mut f) = std::fs::File::create(&tmp) {
                        let _ = f.write_all(&bytes);
                        self.file_path = tmp.display().to_string();
                        match parse_vingen4_file(&self.file_path) {
                            Some(entries) => {
                                self.vingen4_entries = Some(entries);
                                self.file_error = None;
                                self.last_source = LastSource::File;
                            }
                            None => {
                                self.file_error = Some("No VIN data found in file".to_string());
                                self.vingen4_entries = None;
                            }
                        }
                    }
                    break;
                }
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_space(8.0);

                    // File Loading Section
                    egui::Frame::new()
                        .inner_margin(12.0)
                        .outer_margin(4.0)
                        .corner_radius(2.0)
                        .fill(egui::Color32::from_rgb(45, 45, 47))
                        .stroke(egui::Stroke::new(
                            3.0,
                            egui::Color32::from_rgb(100, 100, 105),
                        ))
                        .show(ui, |ui| {
                            ui.heading("⚙ File Loading");
                            ui.add_space(4.0);
                            ui.label(
                                "Drop carparts.txt onto the window to open or specify path below:",
                            );
                            ui.add_space(4.0);
                            ui.add(
                                egui::TextEdit::singleline(&mut self.file_path)
                                    .desired_width(f32::INFINITY),
                            );
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                if ui
                                    .button("Browse...")
                                    .on_hover_text("Select carparts.txt file")
                                    .clicked()
                                {
                                    if let Some(path) = rfd::FileDialog::new()
                                        .add_filter("Text files", &["txt"])
                                        .set_file_name("carparts.txt")
                                        .pick_file()
                                    {
                                        self.file_path = path.display().to_string();
                                    }
                                }
                                if ui
                                    .button("Reset")
                                    .on_hover_text("Reset to default path")
                                    .clicked()
                                {
                                    self.file_path = VinApp::default_file_path();
                                    self.file_error = None;
                                }
                                if ui
                                    .add(
                                        egui::Button::new("Load")
                                            .fill(egui::Color32::from_rgb(200, 120, 40)),
                                    )
                                    .on_hover_text("Load VIN data from file")
                                    .clicked()
                                {
                                    let path = self.file_path.clone();
                                    if !std::path::Path::new(&path).exists() {
                                        self.file_error = Some(format!("File not found: {}", path));
                                        self.vingen4_entries = None;
                                    } else {
                                        match parse_vingen4_file(&path) {
                                            Some(entries) => {
                                                self.vingen4_entries = Some(entries);
                                                self.file_error = None;
                                                self.last_source = LastSource::File;
                                            }
                                            None => {
                                                self.file_error =
                                                    Some("No VIN data found in file".to_string());
                                                self.vingen4_entries = None;
                                            }
                                        }
                                    }
                                }
                            });

                            if let Some(ref err) = self.file_error {
                                ui.add_space(4.0);
                                egui::Frame::new()
                                    .inner_margin(8.0)
                                    .corner_radius(4.0)
                                    .fill(egui::Color32::from_rgb(80, 20, 20))
                                    .show(ui, |ui| {
                                        ui.colored_label(
                                            egui::Color32::from_rgb(255, 100, 100),
                                            err,
                                        );
                                    });
                            }
                        });

                    ui.add_space(12.0);

                    // VIN Input Section
                    egui::Frame::new()
                        .inner_margin(12.0)
                        .outer_margin(4.0)
                        .corner_radius(2.0)
                        .fill(egui::Color32::from_rgb(45, 45, 47))
                        .stroke(egui::Stroke::new(
                            3.0,
                            egui::Color32::from_rgb(100, 100, 105),
                        ))
                        .show(ui, |ui| {
                            ui.heading("✏ Manual VIN Input");
                            ui.add_space(4.0);
                            let vin_len: usize = VIN_STRUCTURE.iter().map(|f| f.len).sum();
                            let mut decode_clicked = false;
                            let vin_input_response = ui.add(
                                egui::TextEdit::singleline(&mut self.vin_input)
                                    .desired_width(f32::INFINITY)
                                    .hint_text("Enter VIN code here..."),
                            );
                            ui.add_space(4.0);
                            if ui
                                .add(
                                    egui::Button::new("Decode")
                                        .fill(egui::Color32::from_rgb(200, 120, 40)),
                                )
                                .on_hover_text("Decode the entered VIN")
                                .clicked()
                            {
                                decode_clicked = true;
                            }
                            if (vin_input_response.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                                || decode_clicked
                            {
                                let vin = self.vin_input.trim().replace(' ', "").to_uppercase();
                                if vin.len() != vin_len {
                                    self.vin_error = Some(format!(
                                        "Invalid VIN length: {} characters (expected {})",
                                        vin.len(),
                                        vin_len
                                    ));
                                    self.entries = None;
                                } else {
                                    self.entries = Some(parse_vin(&vin));
                                    self.vin_error = None;
                                }
                                self.last_source = LastSource::Vin;
                            }
                        });

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);

                    match self.last_source {
                        LastSource::File => {
                            if let Some(ref entries) = self.vingen4_entries {
                                let entry_map: HashMap<_, _> = entries
                                    .iter()
                                    .map(|(k, v)| {
                                        let val = if v.starts_with("string(") && v.ends_with(")") {
                                            &v[7..v.len() - 1]
                                        } else {
                                            &**v
                                        };
                                        (&**k, val)
                                    })
                                    .collect();
                                render_vin_table(ui, &self.decode_map, |key| {
                                    entry_map.get(key).copied().unwrap_or(EMPTY)
                                });
                            }
                        }
                        LastSource::Vin => {
                            if let Some(ref err) = self.vin_error {
                                ui.add_space(8.0);
                                egui::Frame::new()
                                    .inner_margin(8.0)
                                    .corner_radius(4.0)
                                    .fill(egui::Color32::from_rgb(80, 20, 20))
                                    .show(ui, |ui| {
                                        ui.vertical_centered(|ui| {
                                            ui.colored_label(
                                                egui::Color32::from_rgb(255, 100, 100),
                                                err,
                                            );
                                        });
                                    });
                                ui.add_space(8.0);
                            }
                            if let Some(ref entries) = self.entries {
                                render_vin_table(ui, &self.decode_map, |key| {
                                    entries.get(key).map_or(EMPTY, |s| s)
                                });
                            }
                        }
                        LastSource::None => {}
                    }
                });
        });
    }
}

/// Load application icon
fn load_icon() -> Option<egui::IconData> {
    let icon_bytes = include_bytes!("../assets/icon.ico");
    match image::load_from_memory(icon_bytes) {
        Ok(image) => {
            let rgba = image.to_rgba8();
            let (width, height) = rgba.dimensions();
            Some(egui::IconData {
                rgba: rgba.into_raw(),
                width,
                height,
            })
        }
        Err(e) => {
            eprintln!("Warning: Failed to load icon: {}", e);
            None
        }
    }
}

/// Entry point
fn main() {
    let initial_size = egui::vec2(520.0, 960.0);
    let min_size = egui::vec2(520.0, 960.0);

    // Load icon
    let icon_data = load_icon();

    let mut viewport_builder = egui::ViewportBuilder::default()
        .with_inner_size(initial_size)
        .with_min_inner_size(min_size)
        .with_resizable(true);

    if let Some(icon) = icon_data {
        viewport_builder = viewport_builder.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport: viewport_builder,
        ..Default::default()
    };
    let version = env!("CARGO_PKG_VERSION");
    let title = format!("My Winter Car VIN Decoder v{}", version);
    eframe::run_native(
        &title,
        options,
        Box::new(|_cc| Ok(Box::new(VinApp::default()))),
    )
    .expect("Failed to start eframe application");
}
