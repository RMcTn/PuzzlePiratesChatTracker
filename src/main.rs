use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::text::LayoutJob;
use egui::{Color32, Context, FontId, TextFormat, Ui};
use regex::{Captures, Regex};
use time::macros::format_description;
use time::{Date, Time};

const PIRATE_INFO_URL: &'static str = "https://emerald.puzzlepirates.com/yoweb/pirate.wm?target=";

#[derive(Debug)]
struct Battle {
    _id: u32,
    attacker_ship: String,
    defender_ship: String,
    greedies: BTreeMap<String, u32>,
}

#[derive(Debug)]
struct ParsedStuff {
    battles: VecDeque<Battle>,
    chat_messages: Vec<Message>,
    tells: Vec<Message>,
    trade_chat_messages: Vec<Message>,
    global_chat_messages: Vec<Message>,
    messages_with_search_term: Vec<Message>,
    last_line_read: usize,
    total_lines_read: usize,
    // NOTE: Saying this is optional for now. Haven't thought enough about it
    current_date: Option<Date>,
    in_battle: bool,
}

impl ParsedStuff {
    fn new() -> Self {
        return ParsedStuff {
            battles: VecDeque::new(),
            chat_messages: vec![],
            tells: vec![],
            trade_chat_messages: vec![],
            global_chat_messages: vec![],
            messages_with_search_term: vec![],
            last_line_read: 0,
            total_lines_read: 0,
            current_date: None,
            in_battle: false,
        };
    }
}

#[derive(Debug, PartialEq, Clone)]
struct Message {
    timestamp: Time,
    contents: String,
    sender: String,
    // Need to decide what a message that has no date means for sorting on search results.
    date: Option<Date>,
}

impl Message {
    fn new(contents: String, sender: String, timestamp: Time) -> Self {
        return Message {
            contents,
            sender,
            timestamp,
            date: None,
        };
    }

    fn sender_indexes(&self) -> (usize, usize) {
        let sender_start = self.contents.find(&self.sender).unwrap();
        let sender_end = sender_start + self.sender.len();
        return (sender_start, sender_end);
    }

    fn timestamp_from_message(&self) -> &str {
        return &self.contents[0..=self.contents.find("]").unwrap()];
    }

    fn contents_after_sender(&self) -> &str {
        let sender_end_index = self.sender_indexes().1;
        return &self.contents[sender_end_index..self.contents.len()];
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
    // TODO: Combined chat tab?
    // TODO: Alert/Sound/Notification on chat containing search term
    // TODO: Text should be selectable in chat tabs at least
    // TODO: Force a reparse when search term updates (with debounce period?)
    // TODO: Wrap message text (just overflows window at the moment)
    // TODO: Look into the invalid utf-8 errors we get from the chat log, might be useful encoded data?

    let chat_log_path = Arc::new(Mutex::new(None));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default(),
        ..Default::default()
    };

    let parsed_stuff = Arc::new(Mutex::new(ParsedStuff::new()));

    let config_path = Path::new("greedy-tracker.conf");

    if let Ok(contents) = fs::read_to_string(config_path) {
        // TODO: FIXME: Don't assume only the path is there for
        *chat_log_path.lock().unwrap() = Some(Path::new(&contents).to_path_buf());
    };

    let mut selected_panel = Tabs::GreedyHits;
    let mut last_reparse = Instant::now();
    let timer_threshold = Duration::from_millis(2000);

    let search_term = Arc::new(Mutex::new(String::new()));

    if let Some(path) = chat_log_path.lock().unwrap().as_ref() {
        let reader = open_chat_log(path);
        parse_chat_log(
            reader,
            &search_term.lock().unwrap(),
            &mut parsed_stuff.lock().unwrap(),
        );
    }

    let eframe_ctx = Arc::new(Mutex::new(None::<Context>));

    {
        let chat_log_path = chat_log_path.clone();
        let parsed_stuff = parsed_stuff.clone();
        let eframe_ctx = eframe_ctx.clone();
        let search_term = search_term.clone();

        std::thread::spawn(move || loop {
            let now = Instant::now();
            let time_since_last_reparse = now - last_reparse;
            if time_since_last_reparse > timer_threshold {
                dbg!("Reparsing");
                if let Some(path) = chat_log_path.lock().unwrap().as_ref() {
                    let reader = open_chat_log(path);
                    parse_chat_log(
                        reader,
                        &search_term.lock().unwrap(),
                        &mut parsed_stuff.lock().unwrap(),
                    );
                    match eframe_ctx.lock().unwrap().as_ref() {
                        Some(ctx) => ctx.request_repaint(),
                        None => (),
                    }
                    last_reparse = Instant::now();
                }
            }
            std::thread::sleep(Duration::from_millis(500));
        });
    }

    let chat_log_path = chat_log_path.clone();
    let parsed_stuff = parsed_stuff.clone();
    let mut ctx_been_cloned = false;
    eframe::run_simple_native("Greedy tracker", options, move |ctx, _frame| {
        if !ctx_been_cloned {
            *eframe_ctx.lock().unwrap() = Some(ctx.clone());
            ctx_been_cloned = true;
        }

        egui::CentralPanel::default().show(ctx, |mut ui| {
            if ui.button("Open chat log").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_file() {
                    // Wipe our progress on reload
                    *chat_log_path.lock().unwrap() = Some(path.clone());

                    if let Ok(mut file) = File::create(config_path) {
                        file.write_all(path.to_string_lossy().as_bytes()).unwrap();
                    } else {
                        eprintln!(
                            "Couldn't open config file at {}",
                            config_path.to_string_lossy()
                        );
                    }
                }

                // TODO: Drag and drop file
            }
            if ui.button("Reload chat log").clicked() {
                if let Some(path) = chat_log_path.lock().unwrap().as_ref() {
                    let reader = open_chat_log(path);
                    // Wipe our progress on reload
                    *parsed_stuff.lock().unwrap() = ParsedStuff::new();
                    //  TODO: Might want to send a message to the background thread instead of doing this parse here
                    parse_chat_log(
                        reader,
                        &search_term.lock().unwrap(),
                        &mut parsed_stuff.lock().unwrap(),
                    );
                }
            }

            if ui.ctx().has_requested_repaint() {}

            ui.horizontal(|ui| {
                ui.selectable_value(&mut selected_panel, Tabs::GreedyHits, "Greedies");
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
                ui.selectable_value(&mut selected_panel, Tabs::SearchTerm, "Search term");
            });

            match selected_panel {
                Tabs::GreedyHits => greedy_ui(&mut ui, &parsed_stuff.lock().unwrap()),
                Tabs::Chat(chat_type) => chat_ui(&mut ui, &parsed_stuff.lock().unwrap(), chat_type),
                Tabs::SearchTerm => search_chat_ui(
                    &mut ui,
                    &parsed_stuff.lock().unwrap(),
                    &mut search_term.lock().unwrap(),
                ),
            }
        });
    })
    .unwrap();
}

fn search_chat_ui(ui: &mut Ui, parsed_stuff: &ParsedStuff, search_term: &mut String) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.heading("Filtered chat");
        let search_label = ui.label("Search term");
        ui.text_edit_singleline(search_term)
            .labelled_by(search_label.id);

        if parsed_stuff.messages_with_search_term.is_empty() {
            ui.label("No chat messages found.");
        } else {
        }
        for (i, message) in parsed_stuff
            .messages_with_search_term
            .iter()
            .rev()
            .enumerate()
        {
            let message_limit = 100;
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

fn colorize_message(message: &Message) -> LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let sender_indices = message.sender_indexes();
    let sender_start = sender_indices.0;
    let sender_end = sender_indices.1;
    job.append(&message.contents[0..sender_start], 0.0, TextFormat {
        font_id: FontId::default(),
        color: Color32::DARK_GRAY,
        ..Default::default()
    });
    job.append(&message.contents[sender_start..sender_end], 0.0, TextFormat {
        font_id: FontId::default(),
        color: Color32::BLUE,
        ..Default::default()
    });
    job.append(&message.contents[sender_end..message.contents.len()], 0.0, TextFormat {
        font_id: FontId::default(),
        color: Color32::DARK_GRAY,
        ..Default::default()
    });
    return job;
}

fn chat_ui(ui: &mut Ui, parsed_stuff: &ParsedStuff, chat_type: ChatType) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        let heading = match chat_type {
            ChatType::Chat => "Chat",
            ChatType::Trade => "Trade chat",
            ChatType::Global => "Global chat",
            ChatType::Tell => "Tells",
        };
        ui.heading(heading);
        let messages = match chat_type {
            ChatType::Chat => &parsed_stuff.chat_messages,
            ChatType::Trade => &parsed_stuff.trade_chat_messages,
            ChatType::Global => &parsed_stuff.global_chat_messages,
            ChatType::Tell => &parsed_stuff.tells,
        };
        if messages.is_empty() {
            ui.label("No chat messages found.");
        }

        for (i, message) in messages.iter().rev().enumerate() {
            let message_limit = 100;
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
    let job = colorize_message(&message);
    let text = ui.fonts(|f| f.layout_job(job));
    ui.label(text);
}

fn append_player_chat_line(message: &Message, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label(message.timestamp_from_message());
        ui.label(" ");
        ui.hyperlink_to(&message.sender, PIRATE_INFO_URL.to_owned() + &message.sender);
        ui.label(message.contents_without_sender());
    });
}

fn greedy_ui(ui: &mut Ui, parsed_stuff: &ParsedStuff) {
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

fn parse_chat_log<R: Read>(
    buf_reader: BufReader<R>,
    search_string: &str,
    parsed: &mut ParsedStuff,
) {
    // TODO: NOTE: We don't have to go through the entire file again, just what has changed?
    // TODO: Add some configurable limit of how many lines to look back on.
    let lines = buf_reader.lines();
    let mut battle_count = 0;

    let timestamp_regex = r"\[(\d\d:\d\d:\d\d)\]".to_string();
    let sender_section_for_regex = r" (\w+( |-*)?\w+)".to_string();
    let regex_bits = timestamp_regex + &sender_section_for_regex;
    let chat_line_regex = Regex::new(&(regex_bits.clone() + " says,")).unwrap();
    let trade_chat_line_regex = Regex::new(&(regex_bits.clone() + " trade chats,")).unwrap();
    let global_chat_line_regex = Regex::new(&(regex_bits.clone() + " global chats,")).unwrap();
    let tell_chat_line_regex = Regex::new(&(regex_bits.clone() + " tells ye,")).unwrap();

    let date_seperator_regex = Regex::new(r"={5} (\d\d\d\d/\d\d/\d\d) ={5}").unwrap();
    let date_format = format_description!("[year]/[month]/[day]");

    let starting_line = parsed.last_line_read;
    // FIXME(?): TODO: Assuming the chat log will never be pruned or truncated in some way whilst the parser is running. Otherwise our starting line could be beyond what the file's actual size is now. We could warn the user, if we kept track of how many lines we've seen in this parse attempt (couldn't just skip x lines any more, we'd need to iterate through everything and sum it up, but might not actually matter performance wise to count per line), and compare it to total_lines_read (if < warn user)

    for line in lines.skip(starting_line) {
        parsed.last_line_read += 1;
        parsed.total_lines_read += 1;

        if line.is_err() {
            // TODO: Investigate what invalid utf8 we'd actually get
            continue;
        }
        let line = line.unwrap();

        if let Some(captures) = date_seperator_regex.captures(&line) {
            let date = &captures[1];
            let date = Date::parse(date, &date_format).unwrap();
            parsed.current_date = Some(date);
        }

        // TODO: FIXME: It may be possible for a chat message to span multiple lines
        //      a chat from a player will end in a ", even if it's over multiple lines
        if let Some(mut message) = is_chat_line(&line, &chat_line_regex) {
            message.date = parsed.current_date;
            parsed.chat_messages.push(message);
            continue;
        }

        if let Some(mut message) = is_trade_chat_line(&line, &trade_chat_line_regex) {
            message.date = parsed.current_date;
            parsed.trade_chat_messages.push(message);
            continue;
        }

        if let Some(mut message) = is_global_chat_line(&line, &global_chat_line_regex) {
            message.date = parsed.current_date;
            parsed.global_chat_messages.push(message);
            continue;
        }

        if let Some(mut message) = is_tell_chat_line(&line, &tell_chat_line_regex) {
            message.date = parsed.current_date;
            parsed.tells.push(message);
            continue;
        }

        if is_battle_started_line(&line) {
            let splits: Vec<&str> = line.split(" ").collect();
            // TODO: Would like ship/battle naming to be better, but it works
            let attacker_ship = splits[1].to_string() + " " + splits[2];
            let defender_ship = splits[5].to_string() + " " + splits[6];
            parsed.in_battle = true;
            battle_count += 1;
            let battle = Battle {
                _id: battle_count,
                greedies: BTreeMap::new(),
                defender_ship,
                attacker_ship,
            };
            parsed.battles.push_front(battle);
            continue;
        }

        if parsed.in_battle && is_a_greedy_line(&line) {
            let splits: Vec<&str> = line.split(" ").collect();
            let pirate_name = splits[1];
            let battle: &mut Battle = parsed.battles.front_mut().unwrap();
            let greedies = &mut battle.greedies;

            *greedies.entry(pirate_name.to_string()).or_default() += 1;
        }

        if !parsed.in_battle && is_a_greedy_line(&line) {
            dbg!("Processing greedy line, but program believes we're outside of battle!");
        }

        if is_battle_ended_line(&line) {
            parsed.in_battle = false;
        }
    }

    if !search_string.is_empty() {
        for msg in &parsed.chat_messages {
            if msg
                .contents
                .to_lowercase()
                .contains(&search_string.to_lowercase())
            {
                parsed.messages_with_search_term.push(msg.clone());
            }
        }
        for msg in &parsed.trade_chat_messages {
            if msg
                .contents
                .to_lowercase()
                .contains(&search_string.to_lowercase())
            {
                parsed.messages_with_search_term.push(msg.clone());
            }
        }
        for msg in &parsed.global_chat_messages {
            if msg
                .contents
                .to_lowercase()
                .contains(&search_string.to_lowercase())
            {
                parsed.messages_with_search_term.push(msg.clone());
            }
        }
        for msg in &parsed.tells {
            if msg
                .contents
                .to_lowercase()
                .contains(&search_string.to_lowercase())
            {
                parsed.messages_with_search_term.push(msg.clone());
            }
        }
    }
}

fn is_a_greedy_line(string: &str) -> bool {
    return string.contains("delivers a")
        || string.contains("performs a")
        || string.contains("executes a")
        || string.contains("swings a");
}

fn is_battle_ended_line(string: &str) -> bool {
    return string.contains("Game Over");
}

fn is_battle_started_line(string: &str) -> bool {
    return string.contains("A melee breaks out between the crews");
}

/// Timestamp should be in the format [hour:minute:second]
fn get_time_from_timestamp(timestamp: &str) -> Time {
    let timestamp_format = format_description!("[hour]:[minute]:[second]");
    // TODO: Handle timestamp parse failure
    let timestamp = time::Time::parse(&timestamp, &timestamp_format).unwrap();
    return timestamp;
}

fn message_from_captures(captures: &Captures, chat_message: &str) -> Message {
    let timestamp = captures[1].to_string();
    let name = captures[2].to_string();
    return Message::new(
        chat_message.to_string(),
        name,
        get_time_from_timestamp(&timestamp),
    );
}

fn is_chat_line(string: &str, regex: &Regex) -> Option<Message> {
    if let Some(captures) = regex.captures(string) {
        return Some(message_from_captures(&captures, string));
    } else {
        return None;
    }
}

fn is_trade_chat_line(string: &str, regex: &Regex) -> Option<Message> {
    if let Some(captures) = regex.captures(string) {
        return Some(message_from_captures(&captures, string));
    } else {
        return None;
    }
}

fn is_global_chat_line(string: &str, regex: &Regex) -> Option<Message> {
    if let Some(captures) = regex.captures(string) {
        return Some(message_from_captures(&captures, string));
    } else {
        return None;
    }
}

fn is_tell_chat_line(string: &str, regex: &Regex) -> Option<Message> {
    if let Some(captures) = regex.captures(string) {
        return Some(message_from_captures(&captures, string));
    } else {
        return None;
    }
}

#[derive(PartialEq, Copy, Clone)]
enum Tabs {
    GreedyHits,
    Chat(ChatType),
    SearchTerm,
}

#[derive(PartialEq, Copy, Clone)]
enum ChatType {
    Chat,
    Trade,
    Global,
    Tell,
}

#[cfg(test)]
mod tests {
    use std::io::BufReader;

    use time::Month;

    use crate::{is_a_greedy_line, is_battle_started_line, parse_chat_log, ParsedStuff};

    // TODO: Feels like we're testing the same thing over and over for each chat type, but they do have different regexes, so..?

    #[test]
    fn test_greedy_line() {
        let str = "[01:50:54] Bob delivers an overwhelming barrage against Petty Robert, causing some treasure to fall from their grip";
        assert_eq!(is_a_greedy_line(str), true);
    }

    #[test]
    fn test_battle_started() {
        let str = "[02:01:19] Mean Shad has grappled Shifty Shiner. A melee breaks out between the crews!";
        assert_eq!(is_battle_started_line(str), true);
    }

    #[test]
    fn test_regular_chat_line() {
        let single_name_string = "[16:05:01] Someone says, \"we just got intercepted\"\"";
        let double_name_string = "[16:05:01] NPC Name says, \"we just got intercepted\"";

        let log = format!("{}\n{}", single_name_string, double_name_string);

        let reader = BufReader::new(log.as_bytes());
        let mut parsed = ParsedStuff::new();
        parse_chat_log(reader, "", &mut parsed);
        assert_eq!(parsed.chat_messages.len(), 2);
        assert_eq!(parsed.chat_messages[0].contents, single_name_string);
        assert_eq!(parsed.chat_messages[0].sender, "Someone");
        assert_eq!(parsed.chat_messages[1].contents, double_name_string);
        assert_eq!(parsed.chat_messages[1].sender, "NPC Name");
    }

    #[test]
    fn test_timestamp_parsing() {
        let log = "[16:05:01] Someone says, \"we just got intercepted\"\"";
        let reader = BufReader::new(log.as_bytes());
        let mut parsed = ParsedStuff::new();

        parse_chat_log(reader, "", &mut parsed);
        let time = parsed.chat_messages[0].timestamp;
        assert_eq!(time.hour(), 16);
        assert_eq!(time.minute(), 05);
        assert_eq!(time.second(), 01);
    }

    #[test]
    fn test_date_parsing() {
        let chat_string = "[16:05:01] Someone says, \"we just got intercepted\"\"";
        let date_string = "===== 2024/01/06 =====";
        let other_chat_string = "[16:05:05] Someone-else says, \"we just got intercepted\"\"";
        let log = format!("{}\n{}\n{}", chat_string, date_string, other_chat_string);
        let reader = BufReader::new(log.as_bytes());
        let mut parsed = ParsedStuff::new();

        parse_chat_log(reader, "", &mut parsed);
        assert_eq!(parsed.chat_messages[0].date, None);
        let date = parsed.chat_messages[1].date.unwrap();
        assert_eq!(date.year(), 2024);
        assert_eq!(date.month(), Month::January);
        assert_eq!(date.day(), 06);
    }

    #[test]
    fn test_hypen_name() {
        let log = "[16:05:01] Someone-else says, \"we just got intercepted\"\"";
        let reader = BufReader::new(log.as_bytes());
        let mut parsed = ParsedStuff::new();
        parse_chat_log(reader, "", &mut parsed);

        assert_eq!(parsed.chat_messages[0].contents, log);
        assert_eq!(parsed.chat_messages[0].sender, "Someone-else");
    }

    #[test]
    fn test_trade_chat_line() {
        let single_name_string =
            "[16:05:04] Someone trade chats, \"? Buying weavery or plot on barb or arakoua\"";
        let double_name_string =
            "[16:05:04] Big Barry trade chats, \"? Buying weavery or plot on barb or arakoua\"";

        let log = format!("{}\n{}", single_name_string, double_name_string);
        let reader = BufReader::new(log.as_bytes());

        let mut parsed = ParsedStuff::new();
        parse_chat_log(reader, "", &mut parsed);
        assert_eq!(parsed.trade_chat_messages.len(), 2);
        assert_eq!(parsed.trade_chat_messages[0].contents, single_name_string);
        assert_eq!(parsed.trade_chat_messages[0].sender, "Someone");
        assert_eq!(parsed.trade_chat_messages[1].contents, double_name_string);
        assert_eq!(parsed.trade_chat_messages[1].sender, "Big Barry");
    }

    #[test]
    fn test_global_chat_line() {
        let single_name_string = "[16:05:04] Someone global chats, \"2 for spades\"";
        let double_name_string = "[16:05:04] Big Barry global chats, \"2 for spades\"";

        let log = format!("{}\n{}", single_name_string, double_name_string);
        let reader = BufReader::new(log.as_bytes());

        let mut parsed = ParsedStuff::new();
        parse_chat_log(reader, "", &mut parsed);
        assert_eq!(parsed.global_chat_messages.len(), 2);
        assert_eq!(parsed.global_chat_messages[0].contents, single_name_string);
        assert_eq!(parsed.global_chat_messages[0].sender, "Someone");
        assert_eq!(parsed.global_chat_messages[1].contents, double_name_string);
        assert_eq!(parsed.global_chat_messages[1].sender, "Big Barry");
    }

    #[test]
    fn test_tell_chat_line() {
        let single_name_string = "[16:05:04] Someone tells ye, \"2 for spades\"";
        let double_name_string = "[16:05:04] Big Barry tells ye, \"2 for spades\"";

        let log = format!("{}\n{}", single_name_string, double_name_string);
        let reader = BufReader::new(log.as_bytes());

        let mut parsed = ParsedStuff::new();
        parse_chat_log(reader, "", &mut parsed);
        assert_eq!(parsed.tells.len(), 2);
        assert_eq!(parsed.tells[0].contents, single_name_string);
        assert_eq!(parsed.tells[0].sender, "Someone");
        assert_eq!(parsed.tells[1].contents, double_name_string);
        assert_eq!(parsed.tells[1].sender, "Big Barry");
    }

    #[test]
    fn test_lines_read_count() {
        let single_name_string = "[16:05:04] Someone tells ye, \"2 for spades\"";
        let mut log = format!(
            "{}\n{}\n{}\n{}",
            single_name_string, single_name_string, single_name_string, single_name_string
        );
        let reader = BufReader::new(log.as_bytes());
        let mut parsed = ParsedStuff::new();
        parse_chat_log(reader, "", &mut parsed);

        assert_eq!(parsed.last_line_read, 4);
        assert_eq!(parsed.total_lines_read, 4);

        log += "\n[16:05:04] Someone tells ye, \"2 for spades\"";
        let reader = BufReader::new(log.as_bytes());
        let mut parsed = ParsedStuff::new();
        parse_chat_log(reader, "", &mut parsed);
        assert_eq!(parsed.last_line_read, 5);
        assert_eq!(parsed.total_lines_read, 5);
    }

    #[test]
    fn test_battle_in_progress_updated_on_reparse() {
        let battle_started = "[02:01:19] Mean Shad has grappled Shifty Shiner. A melee breaks out between the crews!";
        let first_greedy_hit = "[01:50:54] Bob delivers an overwhelming barrage against Petty Robert, causing some treasure to fall from their grip";
        let mut log = format!("{}\n{}\n", battle_started, first_greedy_hit);

        let reader = BufReader::new(log.as_bytes());
        let mut parsed = ParsedStuff::new();
        parse_chat_log(reader, "", &mut parsed);

        assert_eq!(*parsed.battles[0].greedies.first_key_value().unwrap().1, 1);

        let second_greedy_hit = "[01:50:54] Bob delivers an overwhelming barrage against Petty Robert, causing some treasure to fall from their grip";
        log += second_greedy_hit;
        let reader = BufReader::new(log.as_bytes());
        parse_chat_log(reader, "", &mut parsed);
        assert_eq!(*parsed.battles[0].greedies.first_key_value().unwrap().1, 2);
    }

    // TODO: Some tests that check non matching lines too
    // TODO: Filter test as well
}
