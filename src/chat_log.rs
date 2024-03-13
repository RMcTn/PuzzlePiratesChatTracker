use std::{
    collections::{BTreeMap, VecDeque},
    io::{BufRead, BufReader, Read},
};

use regex::{Captures, Regex};
use time::{macros::format_description, Date, Time};

use crate::{Battle, Message};

#[derive(Debug)]
// TODO: Feels a bit weird that we can create a 'parsed chat log' without actually parsing
// anything. Probably naming issue of parser vs parsed
pub struct ParsedChatLog {
    pub battles: VecDeque<Battle>,
    pub chat_messages: Vec<Message>,
    pub tells: Vec<Message>,
    pub trade_chat_messages: Vec<Message>,
    pub global_chat_messages: Vec<Message>,
    pub last_line_read: usize,
    pub total_lines_read: usize,
    // NOTE: Saying this is optional for now. Haven't thought enough about it
    pub current_date: Option<Date>,
    pub in_battle: bool,
}

impl ParsedChatLog {
    pub fn new() -> Self {
        return ParsedChatLog {
            battles: VecDeque::new(),
            chat_messages: vec![],
            tells: vec![],
            trade_chat_messages: vec![],
            global_chat_messages: vec![],
            last_line_read: 0,
            total_lines_read: 0,
            current_date: None,
            in_battle: false,
        };
    }

    pub fn messages_in_order_of_creation(&self) -> Vec<&Message> {
        let total_message_count = self.chat_messages.len()
            + self.global_chat_messages.len()
            + self.trade_chat_messages.len()
            + self.tells.len();
        let mut messages = Vec::with_capacity(total_message_count);
        for message in &self.chat_messages {
            messages.push(message);
        }
        for message in &self.global_chat_messages {
            messages.push(message);
        }
        for message in &self.trade_chat_messages {
            messages.push(message);
        }
        for message in &self.tells {
            messages.push(message);
        }

        messages.sort_by(|a, b| a.id.cmp(&b.id));

        return messages;
    }

    pub fn messages_containing_search_term(&self, search_string: &str) -> Vec<&Message> {
        // SPEEDUP: Cache the matching messages result so this doesn't trigger every time the UI
        // updates in the search term tab
        let total_message_count = self.chat_messages.len()
            + self.global_chat_messages.len()
            + self.trade_chat_messages.len()
            + self.tells.len();
        let mut messages = Vec::with_capacity(total_message_count);
        if !search_string.is_empty() {
            for msg in &self.chat_messages {
                if msg
                    .contents
                    .to_lowercase()
                    .contains(&search_string.to_lowercase())
                {
                    messages.push(msg);
                }
            }
            for msg in &self.trade_chat_messages {
                if msg
                    .contents
                    .to_lowercase()
                    .contains(&search_string.to_lowercase())
                {
                    messages.push(msg);
                }
            }
            for msg in &self.global_chat_messages {
                if msg
                    .contents
                    .to_lowercase()
                    .contains(&search_string.to_lowercase())
                {
                    messages.push(msg);
                }
            }
            for msg in &self.tells {
                if msg
                    .contents
                    .to_lowercase()
                    .contains(&search_string.to_lowercase())
                {
                    messages.push(msg);
                }
            }
        }

        messages.sort_by(|a, b| a.id.cmp(&b.id));
        return messages;
    }

    pub fn parse_chat_log<R: Read>(&mut self, buf_reader: BufReader<R>) {
        // TODO: NOTE: We don't have to go through the entire file again, just what has changed?
        // TODO: Add some configurable limit of how many lines to look back on.
        let lines = buf_reader.lines();
        let mut battle_count = 0;

        let timestamp_regex = r"\[(\d\d:\d\d:\d\d)\]".to_string();
        let sender_section_for_regex = r" (\w+( |-*)?\w+)".to_string();
        let regex_bits = timestamp_regex + &sender_section_for_regex;
        let chat_line_regex = Regex::new(&(regex_bits.clone() + " (says|shouts),")).unwrap();
        let trade_chat_line_regex = Regex::new(&(regex_bits.clone() + " trade chats,")).unwrap();
        let global_chat_line_regex = Regex::new(&(regex_bits.clone() + " global chats,")).unwrap();
        let tell_chat_line_regex = Regex::new(&(regex_bits.clone() + " tells ye,")).unwrap();

        let date_seperator_regex = Regex::new(r"={5} (\d\d\d\d/\d\d/\d\d) ={5}").unwrap();
        let date_format = format_description!("[year]/[month]/[day]");

        let starting_line = self.last_line_read;
        // FIXME(?): TODO: Assuming the chat log will never be pruned or truncated in some way whilst the parser is running. Otherwise our starting line could be beyond what the file's actual size is now. We could warn the user, if we kept track of how many lines we've seen in this parse attempt (couldn't just skip x lines any more, we'd need to iterate through everything and sum it up, but might not actually matter performance wise to count per line), and compare it to total_lines_read (if < warn user)

        for line in lines.skip(starting_line) {
            self.last_line_read += 1;
            self.total_lines_read += 1;

            // TODO: FIXME: BUG: Message id will increase if a message is multi line (I think), but it still increments so for ordering it works.
            let message_id = self.total_lines_read as u32;
            if line.is_err() {
                // TODO: Investigate what invalid utf8 we'd actually get
                continue;
            }
            let line = line.unwrap();

            if let Some(captures) = date_seperator_regex.captures(&line) {
                let date = &captures[1];
                let date = Date::parse(date, &date_format).unwrap();
                self.current_date = Some(date);
            }

            // TODO: FIXME: It may be possible for a chat message to span multiple lines
            //      a chat from a player will end in a ", even if it's over multiple lines
            if let Some(mut message) = is_chat_line(&line, &chat_line_regex, message_id) {
                message.date = self.current_date;
                self.chat_messages.push(message);
                continue;
            }

            if let Some(mut message) = is_trade_chat_line(&line, &trade_chat_line_regex, message_id)
            {
                message.date = self.current_date;
                self.trade_chat_messages.push(message);
                continue;
            }

            if let Some(mut message) =
                is_global_chat_line(&line, &global_chat_line_regex, message_id)
            {
                message.date = self.current_date;
                self.global_chat_messages.push(message);
                continue;
            }

            if let Some(mut message) = is_tell_chat_line(&line, &tell_chat_line_regex, message_id) {
                message.date = self.current_date;
                self.tells.push(message);
                continue;
            }

            if is_battle_started_line(&line) {
                let splits: Vec<&str> = line.split(' ').collect();
                // TODO: Would like ship/battle naming to be better, but it works
                let attacker_ship = splits[1].to_string() + " " + splits[2];
                let defender_ship = splits[5].to_string() + " " + splits[6];
                self.in_battle = true;
                battle_count += 1;
                let battle = Battle {
                    _id: battle_count,
                    greedies: BTreeMap::new(),
                    defender_ship,
                    attacker_ship,
                };
                self.battles.push_front(battle);
                continue;
            }

            if self.in_battle && is_a_greedy_line(&line) {
                let splits: Vec<&str> = line.split(' ').collect();
                let pirate_name = splits[1];
                let battle: &mut Battle = self.battles.front_mut().unwrap();
                let greedies = &mut battle.greedies;

                *greedies.entry(pirate_name.to_string()).or_default() += 1;
            }

            if !self.in_battle && is_a_greedy_line(&line) {
                dbg!("Processing greedy line, but program believes we're outside of battle!");
            }

            if is_battle_ended_line(&line) {
                self.in_battle = false;
            }
        }

        // TODO: FIXME: Don't just clone these messages (Or at least change their ID)
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
    let timestamp = time::Time::parse(timestamp, &timestamp_format).unwrap();
    return timestamp;
}

fn message_from_captures(captures: &Captures, chat_message: &str, message_id: u32) -> Message {
    let timestamp = captures[1].to_string();
    let name = captures[2].to_string();
    return Message::new(
        chat_message.to_string(),
        name,
        get_time_from_timestamp(&timestamp),
        message_id,
    );
}

fn is_chat_line(string: &str, regex: &Regex, message_id: u32) -> Option<Message> {
    if let Some(captures) = regex.captures(string) {
        return Some(message_from_captures(&captures, string, message_id));
    } else {
        return None;
    }
}

fn is_trade_chat_line(string: &str, regex: &Regex, message_id: u32) -> Option<Message> {
    if let Some(captures) = regex.captures(string) {
        return Some(message_from_captures(&captures, string, message_id));
    } else {
        return None;
    }
}

fn is_global_chat_line(string: &str, regex: &Regex, message_id: u32) -> Option<Message> {
    if let Some(captures) = regex.captures(string) {
        return Some(message_from_captures(&captures, string, message_id));
    } else {
        return None;
    }
}

fn is_tell_chat_line(string: &str, regex: &Regex, message_id: u32) -> Option<Message> {
    if let Some(captures) = regex.captures(string) {
        return Some(message_from_captures(&captures, string, message_id));
    } else {
        return None;
    }
}

#[cfg(test)]
mod tests {
    use std::io::BufReader;

    use time::Month;

    use crate::{
        chat_log::{is_a_greedy_line, is_battle_started_line, ParsedChatLog},
        Message,
    };

    // TODO: Feels like we're testing the same thing over and over for each chat type, but they do have different regexes, so..?

    #[test]
    fn test_greedy_line() {
        let str = "[01:50:54] Bob delivers an overwhelming barrage against Petty Robert, causing some treasure to fall from their grip";
        assert!(is_a_greedy_line(str));
    }

    #[test]
    fn test_battle_started() {
        let str = "[02:01:19] Mean Shad has grappled Shifty Shiner. A melee breaks out between the crews!";
        assert!(is_battle_started_line(str));
    }

    #[test]
    fn test_regular_chat_line() {
        let single_name_string = "[16:05:01] Someone says, \"we just got intercepted\"\"";
        let double_name_string = "[16:05:01] NPC Name says, \"we just got intercepted\"";
        let shout_string = "[16:05:01] Someone shouts, Yeehaw!";

        let log = format!(
            "{}\n{}\n{}",
            single_name_string, double_name_string, shout_string
        );

        let reader = BufReader::new(log.as_bytes());
        let mut parsed = ParsedChatLog::new();
        parsed.parse_chat_log(reader);
        assert_eq!(parsed.chat_messages.len(), 3);
        assert_eq!(parsed.chat_messages[0].contents, single_name_string);
        assert_eq!(parsed.chat_messages[0].sender, "Someone");
        assert_eq!(parsed.chat_messages[1].contents, double_name_string);
        assert_eq!(parsed.chat_messages[1].sender, "NPC Name");
        assert_eq!(parsed.chat_messages[1].contents, double_name_string);
        assert_eq!(parsed.chat_messages[2].sender, "Someone");
    }

    #[test]
    fn test_timestamp_parsing() {
        let log = "[16:05:01] Someone says, \"we just got intercepted\"\"";
        let reader = BufReader::new(log.as_bytes());

        let mut parsed = ParsedChatLog::new();
        parsed.parse_chat_log(reader);
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

        let mut parsed = ParsedChatLog::new();
        parsed.parse_chat_log(reader);
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
        let mut parsed = ParsedChatLog::new();
        parsed.parse_chat_log(reader);

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

        let mut parsed = ParsedChatLog::new();
        parsed.parse_chat_log(reader);
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

        let mut parsed = ParsedChatLog::new();
        parsed.parse_chat_log(reader);
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

        let mut parsed = ParsedChatLog::new();
        parsed.parse_chat_log(reader);
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
        let mut parsed = ParsedChatLog::new();
        parsed.parse_chat_log(reader);

        assert_eq!(parsed.last_line_read, 4);
        assert_eq!(parsed.total_lines_read, 4);

        log += "\n[16:05:04] Someone tells ye, \"2 for spades\"";
        let reader = BufReader::new(log.as_bytes());
        let mut parsed = ParsedChatLog::new();
        parsed.parse_chat_log(reader);
        assert_eq!(parsed.last_line_read, 5);
        assert_eq!(parsed.total_lines_read, 5);
    }

    #[test]
    fn test_battle_in_progress_updated_on_reparse() {
        let battle_started = "[02:01:19] Mean Shad has grappled Shifty Shiner. A melee breaks out between the crews!";
        let first_greedy_hit = "[01:50:54] Bob delivers an overwhelming barrage against Petty Robert, causing some treasure to fall from their grip";
        let mut log = format!("{}\n{}\n", battle_started, first_greedy_hit);

        let reader = BufReader::new(log.as_bytes());
        let mut parsed = ParsedChatLog::new();
        parsed.parse_chat_log(reader);

        assert_eq!(*parsed.battles[0].greedies.first_key_value().unwrap().1, 1);

        let second_greedy_hit = "[01:50:54] Bob delivers an overwhelming barrage against Petty Robert, causing some treasure to fall from their grip";
        log += second_greedy_hit;
        let reader = BufReader::new(log.as_bytes());
        parsed.parse_chat_log(reader);
        assert_eq!(*parsed.battles[0].greedies.first_key_value().unwrap().1, 2);
    }

    #[test]
    fn test_messages_in_order_of_creation() {
        let global_chat = "[16:05:04] Someone global chats, \"2 for spades\"";
        let other_global_chat = "[16:05:05] Someone global chats, \"5 for shovels\"";
        let trade_chat =
            "[16:05:04] Someone trade chats, \"? Buying weavery or plot on barb or arakoua\"";
        let regular_chat = "[16:05:01] Someone says, \"we just got intercepted\"\"";
        let tell = "[16:05:04] Someone tells ye, \"2 for spades\"";
        let log = format!(
            "{}\n{}\n{}\n{}\n{}\n",
            global_chat, trade_chat, regular_chat, other_global_chat, tell
        );
        let reader = BufReader::new(log.as_bytes());
        let mut parsed = ParsedChatLog::new();
        parsed.parse_chat_log(reader);
        let messages = parsed.messages_in_order_of_creation();

        let expected_order = [
            global_chat,
            trade_chat,
            regular_chat,
            other_global_chat,
            tell,
        ];

        assert_eq!(messages.len(), expected_order.len());

        for (message, expected) in messages.iter().zip(expected_order) {
            assert_eq!(message.contents, expected);
        }
    }

    // TODO: Some tests that check non matching lines too
    // TODO: Filter test as well
}
