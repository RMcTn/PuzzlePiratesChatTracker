use std::collections::BTreeMap;
use std::fs;
use std::fs::File;
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chat_log::ParsedChatLog;
use eframe::egui::ViewportBuilder;
use egui::{Context, Ui};
use serde::{Deserialize, Serialize};
use time::{Date, Time};

const PIRATE_INFO_URL: &str = "https://emerald.puzzlepirates.com/yoweb/pirate.wm?target=";

mod chat_log;

#[derive(Deserialize, Serialize, Clone, Debug)]
struct Config {
    chat_log_path: Option<PathBuf>,
    #[serde(default)]
    message_limit: MessageLimit,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
struct MessageLimit(u64);

impl Default for MessageLimit {
    fn default() -> Self {
        MessageLimit(1000)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            chat_log_path: None,
            message_limit: MessageLimit::default(),
        }
    }
}

#[derive(Debug)]
struct Battle {
    _id: u32,
    attacker_ship: String,
    defender_ship: String,
    greedies: BTreeMap<String, u32>,
}

#[derive(Debug, PartialEq, Clone)]
struct Message {
    id: u32,
    timestamp: Time,
    contents: String,
    sender: String,
    // Need to decide what a message that has no date means for sorting on search results.
    date: Option<Date>,
}

impl Message {
    fn new(contents: String, sender: String, timestamp: Time, id: u32) -> Self {
        return Message {
            id,
            contents,
            sender,
            timestamp,
            date: None,
        };
    }

    fn sender_indexes(&self) -> (usize, usize) {
        // This felt like a bad idea, TBC
        let sender_start = self.contents.find(&self.sender).unwrap();
        let sender_end = sender_start + self.sender.len();
        return (sender_start, sender_end);
    }

    fn timestamp_from_message(&self) -> &str {
        return &self.contents[0..=self.contents.find(']').unwrap()];
    }

    fn contents_without_sender(&self) -> String {
        return self.contents[self.sender_indexes().1..self.contents.len()].to_string();
    }

    /// Players can't have whitespace in their names, but NPCs can.
    /// Not sure if there are NPCs with no whitespace in their names.
    fn is_sender_npc(&self) -> bool {
        return self.sender.split_whitespace().count() > 1;
    }
}

fn main() {
    // TODO: Track personal plunder from battles
    // TODO: Message monitor - look for messages in trade chat like 'message contains BUYING <some text> <item>, but only if the item is before a SELLING word in the same message etc)
    // TODO: Warning if chat log is over a certain size?
    // TODO: Filters for the chat tab? Search by word, pirate name etc - Expand to allow for multiple word searches (allow regex?)
    // TODO: Configurable delay
    // TODO: Error on failed parse (wrong file given for example)
    // TODO: Unread indicator on chat tabs
    // TODO: Alert/Sound/Notification on chat containing search term
    // TODO: Force a reparse when search term updates (with debounce period?)
    // TODO: Look into the invalid utf-8 errors we get from the chat log, might be useful encoded data?
    // TODO: Have the different chat types differ in some way in all chat
    // TODO: Show the date timestamp beside messages (toggleable) - It's handy when looking back at older messages
    // TODO: User settings tab
    // TODO: User settings, let user pick colour for each chat
    // TODO: User settings, increase font size

    let parsed_stuff = Arc::new(Mutex::new(ParsedChatLog::new()));

    let config_path = Path::new("puzzle-pirates-chat-tracker.toml");

    let config: Arc<Mutex<Config>> = Arc::new(Mutex::new(Config::default()));
    if let Ok(contents) = fs::read_to_string(config_path) {
        let parsed_config: Config = toml::from_str(&contents).unwrap();
        *config.lock().unwrap() = parsed_config;
    }

    let mut selected_panel = Tabs::Chat(ChatType::All);
    let mut last_reparse = Instant::now();
    let timer_threshold = Duration::from_millis(2000);

    let search_term = Arc::new(Mutex::new(String::new()));

    if let Some(chat_log_path) = &config.lock().unwrap().chat_log_path {
        let reader = open_chat_log(chat_log_path);
        parsed_stuff.lock().unwrap().parse_chat_log(reader);
    }

    let eframe_ctx = Arc::new(Mutex::new(None::<Context>));

    {
        let config = config.clone();
        let parsed_stuff = parsed_stuff.clone();
        let eframe_ctx = eframe_ctx.clone();

        std::thread::spawn(move || loop {
            let now = Instant::now();
            let time_since_last_reparse = now - last_reparse;
            if time_since_last_reparse > timer_threshold {
                dbg!("Reparsing");
                if let Some(chat_log_path) = &config.lock().unwrap().chat_log_path {
                    let reader = open_chat_log(chat_log_path);
                    parsed_stuff.lock().unwrap().parse_chat_log(reader);
                }
                if let Some(ctx) = eframe_ctx.lock().unwrap().as_ref() {
                    ctx.request_repaint();
                }
                last_reparse = Instant::now();
            }
            std::thread::sleep(Duration::from_millis(500));
        });
    }

    let config = config.clone();
    let parsed_stuff = parsed_stuff.clone();
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default(),
        ..Default::default()
    };
    let mut ctx_been_cloned = false;
    eframe::run_simple_native(
        "Puzzle Pirates Chat Tracker",
        options,
        move |ctx, _frame| {
            if !ctx_been_cloned {
                *eframe_ctx.lock().unwrap() = Some(ctx.clone());
                ctx_been_cloned = true;
            }

            egui::CentralPanel::default().show(ctx, |ui| {
                if ui.button("Open chat log").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_file() {
                        let config = {
                            let mut config = config.lock().unwrap();
                            config.chat_log_path = Some(path);
                            config.clone()
                        };

                        if let Ok(mut file) = File::create(config_path) {
                            let toml = toml::to_string(&config).unwrap();
                            file.write_all(&toml.as_bytes()).unwrap();
                        } else {
                            eprintln!(
                                "Couldn't open config file at {}",
                                config_path.to_string_lossy()
                            );
                        }

                        let mut parsed = parsed_stuff.lock().unwrap();
                        *parsed = ParsedChatLog::new();
                        if let Some(chat_log_path) = &config.chat_log_path {
                            let reader = open_chat_log(chat_log_path);
                            parsed.parse_chat_log(reader);
                        }
                    }

                    // TODO: Drag and drop file
                }
                if ui.button("Reload chat log").clicked() {
                    if let Some(chat_log_path) = &config.lock().unwrap().chat_log_path {
                        let reader = open_chat_log(chat_log_path);
                        // Wipe our progress on reload
                        let mut parsed = parsed_stuff.lock().unwrap();
                        *parsed = ParsedChatLog::new();
                        //  TODO: Might want to send a message to the background thread instead of doing this parse here
                        parsed.parse_chat_log(reader);
                    }
                }

                ui.horizontal(|ui| {
                    ui.selectable_value(&mut selected_panel, Tabs::Chat(ChatType::All), "All chat");
                    ui.selectable_value(&mut selected_panel, Tabs::Chat(ChatType::Chat), "Chat");
                    ui.selectable_value(
                        &mut selected_panel,
                        Tabs::Chat(ChatType::Trade),
                        "Trade chat",
                    );
                    ui.selectable_value(
                        &mut selected_panel,
                        Tabs::Chat(ChatType::Global),
                        "Global chat",
                    );
                    ui.selectable_value(&mut selected_panel, Tabs::Chat(ChatType::Tell), "Tells");
                    ui.selectable_value(&mut selected_panel, Tabs::SearchChat, "Search chat");
                    ui.selectable_value(&mut selected_panel, Tabs::GreedyHits, "Greedies");
                });

                let message_limit = config.lock().unwrap().message_limit.0 as usize;
                match selected_panel {
                    Tabs::GreedyHits => greedy_ui(ui, &parsed_stuff.lock().unwrap()),
                    Tabs::Chat(chat_type) => {
                        chat_ui(ui, &parsed_stuff.lock().unwrap(), chat_type, message_limit)
                    }
                    Tabs::SearchChat => search_chat_ui(
                        ui,
                        &parsed_stuff.lock().unwrap(),
                        &mut search_term.lock().unwrap(),
                        message_limit,
                    ),
                }
            });
        },
    )
    .unwrap();
}

fn search_chat_ui(
    ui: &mut Ui,
    parsed_stuff: &ParsedChatLog,
    search_term: &mut String,
    message_limit: usize,
) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.heading("Filtered chat");
        let search_label = ui.label("Search term");
        ui.text_edit_singleline(search_term)
            .labelled_by(search_label.id);

        let matching_messages = parsed_stuff.messages_containing_search_term(search_term);
        if matching_messages.is_empty() {
            ui.label("No chat messages found.");
        }
        for (i, message) in matching_messages.iter().rev().enumerate() {
            if i >= message_limit {
                break;
            }

            ui.separator();
            if message.is_sender_npc() {
                // Probably an NPC, won't have a pirate page to go to
                append_npc_chat_line(message, ui);
            } else {
                append_player_chat_line(message, ui);
            }
        }
    });
}

fn chat_ui(ui: &mut Ui, parsed_stuff: &ParsedChatLog, chat_type: ChatType, message_limit: usize) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        let heading = match chat_type {
            ChatType::Chat => "Chat",
            ChatType::Trade => "Trade chat",
            ChatType::Global => "Global chat",
            ChatType::Tell => "Tells",
            ChatType::All => "All chat",
        };
        ui.heading(heading);

        if chat_type == ChatType::All {
            for (i, message) in parsed_stuff
                .messages_in_order_of_creation()
                .iter()
                .rev()
                .enumerate()
            {
                if i >= message_limit {
                    break;
                }

                ui.separator();
                if message.is_sender_npc() {
                    // Probably an NPC, won't have a pirate page to go to
                    append_npc_chat_line(message, ui);
                } else {
                    append_player_chat_line(message, ui);
                }
            }
            return;
        }

        let messages = match chat_type {
            ChatType::Chat => &parsed_stuff.chat_messages,
            ChatType::Trade => &parsed_stuff.trade_chat_messages,
            ChatType::Global => &parsed_stuff.global_chat_messages,
            ChatType::Tell => &parsed_stuff.tells,
            ChatType::All => panic!("Shouldn't have reached here"),
        };

        if messages.is_empty() {
            ui.label("No chat messages found.");
        }

        for (i, message) in messages.iter().rev().enumerate() {
            if i >= message_limit {
                break;
            }

            ui.separator();
            if message.is_sender_npc() {
                // Probably an NPC, won't have a pirate page to go to
                append_npc_chat_line(message, ui);
            } else {
                append_player_chat_line(message, ui);
            }
        }
    });
}

fn append_npc_chat_line(message: &Message, ui: &mut Ui) {
    let npc_name_color = egui::Color32::from_hex("#FF4500").unwrap();
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label(message.timestamp_from_message());
        ui.label(" ");
        ui.label(egui::RichText::new(&message.sender).color(npc_name_color));
        ui.add(egui::Label::new(message.contents_without_sender()).wrap(true));
    });
}

fn append_player_chat_line(message: &Message, ui: &mut Ui) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label(message.timestamp_from_message());
        ui.label(" ");
        ui.hyperlink_to(
            &message.sender,
            PIRATE_INFO_URL.to_owned() + &message.sender,
        );
        ui.add(egui::Label::new(message.contents_without_sender()).wrap(true));
    });
}

fn greedy_ui(ui: &mut Ui, parsed_stuff: &ParsedChatLog) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        if parsed_stuff.battles.is_empty() {
            ui.label("No battles detected.");
        } else {
            ui.heading("Greedy hits");
            for battle in &parsed_stuff.battles {
                ui.separator();
                ui.heading(format!(
                    "Battle between {} and {}",
                    battle.attacker_ship, battle.defender_ship
                ));
                let greedy_count: u32 = battle.greedies.values().sum();
                let total_greedy_hits_str = format!("{} Greedies in total", greedy_count);
                ui.label(&total_greedy_hits_str);
                if battle.greedies.is_empty() {
                    ui.label("No Greedies for this battle");
                } else {
                    let mut sorted_results: Vec<(&String, &u32)> = battle.greedies.iter().collect();
                    sorted_results.sort_by(|a, b| b.1.cmp(a.1));

                    let mut greedy_clipboard_text = String::new();
                    greedy_clipboard_text.push_str(&total_greedy_hits_str);
                    greedy_clipboard_text += ". ";

                    for (i, entry) in sorted_results.iter().enumerate() {
                        let s = if i == sorted_results.len() - 1 {
                            format!("{}: {}", entry.0, entry.1)
                        } else {
                            format!("{}: {}, ", entry.0, entry.1)
                        };
                        greedy_clipboard_text.push_str(&s);
                    }

                    if ui.button("Copy me!").clicked() {
                        ui.output_mut(|o| o.copied_text = greedy_clipboard_text);
                    }

                    for entry in &sorted_results {
                        ui.label(format!("{} got {}", entry.0, entry.1));
                    }
                }
            }
        }
    });
}

fn open_chat_log(path: &Path) -> BufReader<File> {
    let file = File::open(path).unwrap();
    return BufReader::new(file);
}

#[derive(PartialEq, Copy, Clone)]
enum Tabs {
    GreedyHits,
    Chat(ChatType),
    SearchChat,
}

#[derive(PartialEq, Copy, Clone)]
enum ChatType {
    Chat,
    Trade,
    Global,
    Tell,
    All,
}
