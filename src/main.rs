use std::collections::BTreeMap;
use std::fs::File;
use std::io;
use std::io::{BufRead, Read};
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
    let mut chat_log_path = None;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default(),
        ..Default::default()
    };

    let mut parsed_stuff = None;

    let mut selected_panel = Tabs::GreedyHits;
    let mut last_reparse = Instant::now();
    let timer_threshold = Duration::from_millis(2000);
    eframe::run_simple_native("Greedy tracker", options, move |ctx, _frame| {
        egui::CentralPanel::default().show(ctx, |mut ui| {
            if ui.button("Open chat log").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_file() {
                    chat_log_path = Some(path);
                }

                // TODO: Drag and drop file
            }
            if ui.button("Reload chat log").clicked() {
                if let Some(path) = &chat_log_path {
                    parsed_stuff = Some(chat_log_stuff(&path));
                }
            }
            if ui.ctx().has_requested_repaint() {
                let now = Instant::now();
                let time_since_last_reparse = now - last_reparse;
                if time_since_last_reparse > timer_threshold {
                    dbg!("Running repaint");
                    if let Some(path) = &chat_log_path {
                        parsed_stuff = Some(chat_log_stuff(path));
                    }
                    last_reparse = Instant::now();
                }
            }

            ui.horizontal(|ui| {
                ui.selectable_value(&mut selected_panel, Tabs::GreedyHits, "Greedies");
                ui.selectable_value(&mut selected_panel, Tabs::Chat, "Chat");
            });

            match selected_panel {
                Tabs::GreedyHits => greedy_ui(&mut ui, parsed_stuff.as_ref()),
                Tabs::Chat => chat_ui(&mut ui, parsed_stuff.as_ref()),
            }
        });
    }).unwrap();
}

fn chat_ui(ui: &mut Ui, parsed_stuff: Option<&ParsedStuff>) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.heading("Chat");
        if let Some(parsed_stuff) = parsed_stuff {
            for (i, message) in parsed_stuff.chat_messages.iter().rev().enumerate() {
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

fn chat_log_stuff(path: &Path) -> ParsedStuff {
    // TODO: NOTE: We don't have to go through the entire file again, just what has changed?
    // TODO: Add some configurable limit of how many lines to look back on.
    let file = File::open(path).unwrap();
    let lines = io::BufReader::new(file).lines();
    let mut in_battle = false;
    let mut battles = vec![];
    let mut battle_count = 0;

    let mut chat_messages = vec![];

    for line in lines {
        if line.is_err() {
            // TODO: Investigate what invalid utf8 we'd actually get
            continue;
        }
        let line = line.unwrap();

        if is_chat_line(&line) {
            // TODO: FIXME: It may be possible for a chat message to span multiple lines
            //      a chat from a player will end in a ", even if it's over multiple lines
            // Skip any more processing, cba with borrow checker atm
            chat_messages.push(line);
            continue;
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
    return string.contains("says");
}

#[derive(PartialEq, Copy, Clone)]
enum Tabs {
    GreedyHits,
    Chat,
}

mod tests {
    use crate::{is_a_greedy_line, is_battle_started_line};

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
}