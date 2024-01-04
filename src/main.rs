use std::{fs, io};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, Read, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use egui::Ui;

#[derive(Debug)]
struct Battle {
    id: u32,
    attacker_ship: String,
    defender_ship: String,
    greedies: BTreeMap<String, u32>,
}

#[derive(Debug)]
struct ParsedStuff {
    battles: Vec<Battle>,
    chat_messages: Vec<String>,
    tells: Vec<String>,
    trade_chat_messages: Vec<String>,
    global_chat_messages: Vec<String>,
    messages_with_search_term: Vec<String>,
}

fn main() {
    // TODO: Do not commit this without removing the username
    // TODO: Track personal plunder from battles
    // TODO: Message monitor - look for messages in trade chat like 'message contains BUYING <some text> <item>, but only if the item is before a SELLING word in the same message etc)
    // TODO: "global chats" tab
    // TODO: "trade chats" tab
    // TODO: "tells chat" tab
    // TODO: Warning if chat log is over a certain size?
    // TODO: Filters for the chat tab? Search by word, pirate name etc
    // TODO: Configurable delay
    // TODO: File picker
    // TODO: Error on failed parse (wrong file given for example)
    // TODO: Unread indicator on chat tabs
    // TODO: Combined chat tab?
    // TODO: Alert/Sound/Notification on chat containing search term
    // TODO: Text should be selectable in chat tabs at least
    // TODO: Colour pirate names in chat tabs?
    // TODO: Tells from NPCs should be handled? These can be multiple words with spaces between for the name before the "tells ye" part
    // TODO: Parse the date from the chat log too (format is "====== 2023/12/27 ======")

    // TODO: FIXME: Make message matching more reliable than just "string contains x" (there is a format to the messages, use that - [timestamp] <pirate-name> says <content> - Likewise with trade + global)
    let mut chat_log_path = None;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default(),
        ..Default::default()
    };

    let mut parsed_stuff = None;

    let config_path = Path::new("greedy-tracker.conf");

    if let Ok(contents) = fs::read_to_string(config_path) {
        // TODO: FIXME: Don't assume only the path is there for
        chat_log_path = Some(Path::new(&contents).to_path_buf());
    };

    let mut selected_panel = Tabs::GreedyHits;
    let mut last_reparse = Instant::now();
    let timer_threshold = Duration::from_millis(2000);

    let mut last_search_term = String::new();
    let mut search_term = String::new();

    if let Some(path) = &chat_log_path {
        parsed_stuff = Some(chat_log_stuff(path, &search_term));
    }

    eframe::run_simple_native("Greedy tracker", options, move |ctx, _frame| {
        egui::CentralPanel::default().show(ctx, |mut ui| {
            if ui.button("Open chat log").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_file() {
                    chat_log_path = Some(path.clone());

                    if let Ok(mut file) = File::create(config_path) {
                        file.write_all(path.to_string_lossy().as_bytes()).unwrap();
                    } else {
                        eprintln!("Couldn't open config file at {}", config_path.to_string_lossy());
                    }
                }

                // TODO: Drag and drop file
            }
            if ui.button("Reload chat log").clicked() {
                if let Some(path) = &chat_log_path {
                    parsed_stuff = Some(chat_log_stuff(&path, &search_term));
                }
            }
            if ui.ctx().has_requested_repaint() {
                let now = Instant::now();
                let time_since_last_reparse = now - last_reparse;
                if time_since_last_reparse > timer_threshold || search_term != last_search_term {
                    dbg!("Running repaint");
                    if let Some(path) = &chat_log_path {
                        parsed_stuff = Some(chat_log_stuff(path, &search_term));
                    }
                    last_reparse = Instant::now();
                }
            }

            ui.horizontal(|ui| {
                ui.selectable_value(&mut selected_panel, Tabs::GreedyHits, "Greedies");
                ui.selectable_value(&mut selected_panel, Tabs::Chat(ChatType::Chat), "Chat");
                ui.selectable_value(&mut selected_panel, Tabs::Chat(ChatType::Trade), "Trade chat");
                ui.selectable_value(&mut selected_panel, Tabs::Chat(ChatType::Global), "Global chat");
                ui.selectable_value(&mut selected_panel, Tabs::Chat(ChatType::Tell), "Tells");
                ui.selectable_value(&mut selected_panel, Tabs::SearchTerm, "Search term");
            });

            match selected_panel {
                Tabs::GreedyHits => greedy_ui(&mut ui, parsed_stuff.as_ref()),
                Tabs::Chat(chat_type) => chat_ui(&mut ui, parsed_stuff.as_ref(), chat_type),
                Tabs::SearchTerm => search_chat_ui(&mut ui, parsed_stuff.as_ref(), &mut search_term),
            }
        });
    }).unwrap();
}

fn search_chat_ui(ui: &mut Ui, parsed_stuff: Option<&ParsedStuff>, mut search_term: &mut String) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.heading("Filtered chat");
        let search_label = ui.label("Search term");
        ui.text_edit_singleline(search_term)
            .labelled_by(search_label.id);

        if let Some(parsed_stuff) = parsed_stuff {
            for (i, message) in parsed_stuff.messages_with_search_term.iter().rev().enumerate() {
                let message_limit = 100;
                if i >= message_limit {
                    break;
                }

                ui.separator();
                ui.label(message);
            }
        } else {
            ui.label("No chat messages found.");
        }
    });
}

fn chat_ui(ui: &mut Ui, parsed_stuff: Option<&ParsedStuff>, chat_type: ChatType) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        let heading = match chat_type {
            ChatType::Chat => "Chat",
            ChatType::Trade => "Trade chat",
            ChatType::Global => "Global chat",
            ChatType::Tell => "Tells",
        };
        ui.heading(heading);
        if let Some(parsed_stuff) = parsed_stuff {
            let messages = match chat_type {
                ChatType::Chat => &parsed_stuff.chat_messages,
                ChatType::Trade => &parsed_stuff.trade_chat_messages,
                ChatType::Global => &parsed_stuff.global_chat_messages,
                ChatType::Tell => &parsed_stuff.tells,
            };
            for (i, message) in messages.iter().rev().enumerate() {
                let message_limit = 100;
                if i >= message_limit {
                    break;
                }

                ui.separator();
                ui.label(message);
            }
        } else {
            ui.label("No chat messages found.");
        }
    });
}

fn greedy_ui(ui: &mut Ui, parsed_stuff: Option<&ParsedStuff>) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.heading("Greedy hits");
        if let Some(parsed_stuff) = parsed_stuff {
            for battle in &parsed_stuff.battles {
                ui.separator();
                ui.heading(format!("Battle between {} and {}", battle.attacker_ship, battle.defender_ship));
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
        } else {
            ui.label("No battles detected.");
        }
    });
}

fn chat_log_stuff(path: &Path, search_string: &str) -> ParsedStuff {
    // TODO: NOTE: We don't have to go through the entire file again, just what has changed?
    // TODO: Add some configurable limit of how many lines to look back on.
    let file = File::open(path).unwrap();
    let lines = io::BufReader::new(file).lines();
    let mut in_battle = false;
    let mut battles = vec![];
    let mut battle_count = 0;

    let mut chat_messages = vec![];
    let mut trade_chat_messages = vec![];
    let mut global_chat_messages = vec![];
    let mut tells = vec![];
    let mut messages_with_search_term = vec![];

    for line in lines {
        if line.is_err() {
            // TODO: Investigate what invalid utf8 we'd actually get
            continue;
        }
        let line = line.unwrap();
        if line.to_lowercase().contains(&search_string.to_lowercase()) {
            // TODO: Stop with the cloning
            messages_with_search_term.push(line.clone());
        }
        // TODO: FIXME: It may be possible for a chat message to span multiple lines
        //      a chat from a player will end in a ", even if it's over multiple lines
        {
            if is_chat_line(&line) {
                // Skip any more processing, cba with borrow checker atm
                chat_messages.push(line);
                continue;
            }

            if is_trade_chat_line(&line) {
                // Skip any more processing, cba with borrow checker atm
                trade_chat_messages.push(line);
                continue;
            }

            if is_global_chat_line(&line) {
                // Skip any more processing, cba with borrow checker atm
                global_chat_messages.push(line);
                continue;
            }

            if is_tell_chat_line(&line) {
                // Skip any more processing, cba with borrow checker atm
                tells.push(line);
                continue;
            }
        }

        if is_battle_started_line(&line) {
            let splits: Vec<&str> = line.split(" ").collect();
            // TODO: Would like ship/battle naming to be better, but it works
            let attacker_ship = splits[1].to_string() + " " + splits[2];
            let defender_ship = splits[5].to_string() + " " + splits[6];
            in_battle = true;
            battle_count += 1;
            let battle = Battle {
                id: battle_count,
                greedies: BTreeMap::new(),
                defender_ship,
                attacker_ship,
            };
            battles.push(battle);
            continue;
        }

        if in_battle && is_a_greedy_line(&line) {
            let splits: Vec<&str> = line.split(" ").collect();
            let pirate_name = splits[1];
            let mut battle: &mut Battle = battles.last_mut().unwrap();
            let mut greedies = &mut battle.greedies;

            *greedies.entry(pirate_name.to_string()).or_default() += 1;
        }

        if !in_battle && is_a_greedy_line(&line) {
            dbg!("Processing greedy line, but program believes we're outside of battle!");
        }

        if is_battle_ended_line(&line) {
            in_battle = false;
        }
    }

    battles.reverse();
    let parsed_stuff = ParsedStuff {
        battles,
        chat_messages,
        trade_chat_messages,
        global_chat_messages,
        tells,
        messages_with_search_term,
    };
    return parsed_stuff;
}


fn is_a_greedy_line(string: &str) -> bool {
    return string.contains("delivers a") || string.contains("performs a") || string.contains("executes a")
        || string.contains("swings a");
}


fn is_battle_ended_line(string: &str) -> bool {
    return string.contains("Game Over");
}

fn is_battle_started_line(string: &str) -> bool {
    return string.contains("A melee breaks out between the crews");
}

fn is_chat_line(string: &str) -> bool {
    return string.contains("says,");
}

fn is_trade_chat_line(string: &str) -> bool {
    return string.contains("trade chats,");
}

fn is_global_chat_line(string: &str) -> bool {
    return string.contains("global chats,");
}

fn is_tell_chat_line(string: &str) -> bool {
    return string.contains("tells ye,");
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

mod tests {
    use crate::{is_a_greedy_line, is_battle_started_line, is_chat_line, is_global_chat_line, is_tell_chat_line, is_trade_chat_line};

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
        let str = "[16:05:01] Someone says, \"we just got intercepted\"";
        assert_eq!(is_chat_line(str), true);
    }

    #[test]
    fn test_trade_chat_line() {
        let str = "[16:05:04] Someone trade chats, \"? Buying weavery or plot on barb or arakoua ?  Selling cookies 250 ea, no pink\"";
        assert_eq!(is_trade_chat_line(str), true);
    }

    #[test]
    fn test_global_chat_line() {
        let str = "[16:03:12] Someone global chats, \"? 2 for spades\"";
        assert_eq!(is_global_chat_line(str), true);

    }

    #[test]
    fn test_tell_chat_line() {
        let str = "[16:03:12] Someone tells ye, \"? 2 for spades\"";
        assert_eq!(is_tell_chat_line(str), true);
    }

    // TODO: Some tests that check non matching lines too
    // TODO: Filter test as well
    // TODO: Maybe just start parsing an actual test chat log and check the resulting parsed struct

}