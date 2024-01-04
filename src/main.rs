use std::collections::{BTreeMap, VecDeque};
use std::fs::File;
use std::io;
use std::io::{BufRead, Read};
use std::path::Path;

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
    // TODO: Copy greedy hits text for battle
    let chat_log_path = Path::new("C:/Users/r/Documents/***REMOVED***_emerald_puzzlepirateslog.txt");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default(),
        ..Default::default()
    };

    let mut parsed_stuff = chat_log_stuff(&chat_log_path);

    eframe::run_simple_native("Greedy tracker", options, move |ctx, _frame| {
        egui::SidePanel::left("greedy_panel").show(ctx, |ui| {
            if ui.button("Reload chat log").clicked() {
                parsed_stuff = chat_log_stuff(&chat_log_path);
            }
            if ui.ctx().has_requested_repaint() {
                // dbg!("Running repaint {}", std::time::Instant::now());
                // parsed_stuff = chat_log_stuff(&chat_log_path, max_lines_to_look_at);
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("My App");
                for battle in &parsed_stuff.battles {
                    ui.separator();
                    ui.heading(format!("Battle between {} and {}", battle.attacker_ship, battle.defender_ship));
                    let greedy_count: u32 = battle.greedies.values().sum();
                    ui.label(format!("{} greedy hits in total", greedy_count));

                    if battle.greedies.is_empty() {
                        ui.label("No greedies for this battle");
                    } else {
                        let mut sorted_results: Vec<(&String, &u32)> = battle.greedies.iter().collect();
                        sorted_results.sort_by(|a, b| b.1.cmp(a.1));

                        for entry in sorted_results {
                            ui.label(format!("{} got {}", entry.0, entry.1));
                        }
                    }
                }
            });

            egui::SidePanel::right("chat_panel").show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.heading("Chat bt");
                    for (i, message) in parsed_stuff.chat_messages.iter().rev().enumerate() {
                        let message_limit = 100;
                        if i >= message_limit {
                            break;
                        }

                        ui.separator();
                        ui.label(message);
                    }
                });
            });
        });
    }).unwrap();
}

fn chat_log_stuff(path: &Path) -> ParsedStuff {
    // TODO: NOTE: We don't have to go through the entire file again, just what has changed?
    // TODO: Add some configurable limit of how many lines to look back on.
    let mut file = File::open(path).unwrap();
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
                defender_ship: defender_ship,
                attacker_ship: attacker_ship,
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

mod tests {
    use crate::{is_a_greedy_line, is_battle_started_line};

    #[test]
    fn test_greedy_line() {
        let str = "[01:50:54] Tamsinlin delivers an overwhelming barrage against Petty Robert, causing some treasure to fall from their grip";
        assert_eq!(is_a_greedy_line(str), true);
    }

    #[test]
    fn test_battle_started() {
        let str = "[02:01:19] Mean Shad has grappled Shifty Shiner. A melee breaks out between the crews!";
        assert_eq!(is_battle_started_line(str), true);
    }
}