## Puzzle Pirates Chat Tracker
I got fed up missing messages in the chat, so here's something that reads your Puzzle Pirates chat log and presents chat messages in a more searchable way.


### Features
- Separate tabs for the different chat types
- Search player and NPC messages across supported chat types
- Check a pirate's page straight from the chat message (Emerald ocean only)
- Simple Greedy hit tracker

#### Supported Chat types
- Trade
- Tells (whispers)
- Global
- Regular chat

### Limitations
- This program was built with Emerald ocean in mind. Other oceans may be supported in future (this mainly affects checking pirate pages)

### Config
Most users won't need to worry about this section. Anything configurable should be editable through the program's UI.  
Configuration values are pulled from the `puzzle-pirates-chat-tracker.toml` file if available. If it isn't available, it will be created by the program.

The configuration format is [TOML](https://toml.io/en/)

##### Config values
| Value | Use | Example |
|-------|-----|----------
| chat_log_path | The location of the chat file to use | TODO


### Building
This section is aimed at developers, or anyone wanting to build the program themselves.

- Install [Rust](https://www.rust-lang.org/learn/get-started)
- Clone this repository
- `cargo run` in the repository folder on your machine
