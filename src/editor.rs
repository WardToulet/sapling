//! The top-level functionality of Sapling

use crate::ast::display_token::DisplayToken;
use crate::ast::{size, Ast};
use crate::editable_tree::{Direction, EditableTree};
use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
use tuikit::prelude::*;

/// The possible log levels
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum LogLevel {
    /// Logs that give lots of minute details.  Intended to be used only when debugging Sapling.
    VerboseDebug = 0,
    /// Less verbose debugging, intended to be used only when debugging Sapling.
    Debug = 1,
    /// Logs that just give information to the user
    Info = 2,
    /// Logs that represent warnings about an Error that is either going to happen or might have
    /// already happened.
    Warning = 3,
    /// Logs that will always be logged.
    Error = 4,
}

impl LogLevel {
    /// Returns a [`Color`] with which to display this log entry
    pub fn to_color(&self) -> Color {
        match self {
            LogLevel::VerboseDebug => Color::BLUE,
            LogLevel::Debug => Color::LIGHT_BLUE,
            LogLevel::Info => Color::GREEN,
            LogLevel::Warning => Color::YELLOW,
            LogLevel::Error => Color::RED,
        }
    }
}

/// The possible command typed by user without any parameters.
/// It can be mapped to a single key.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum Command {
    /// Quit Sapling
    Quit,
    /// Replace the selected node, expects an argument
    Replace,
    /// Insert a new node, expects an argument
    InsertChild,
    /// Move cursor in given direction
    /// This is not considered a parameter as the direction is still specified by pressing specific
    /// key.
    MoveCursor(Direction),
    /// Undo the last change
    Undo,
    /// Redo a change
    Redo,
}

/// Mapping of keys to commands.
/// Shortcut definition, also allows us to change the type if needed.
pub type KeyMap = std::collections::HashMap<char, Command>;

pub fn default_keymap() -> KeyMap {
    hmap::hmap! {
        'q' => Command::Quit,
        'i' => Command::InsertChild,
        'r' => Command::Replace,
        'c' => Command::MoveCursor(Direction::Down),
        'p' => Command::MoveCursor(Direction::Up),
        'k' => Command::MoveCursor(Direction::Prev),
        'j' => Command::MoveCursor(Direction::Next),
        'u' => Command::Undo,
        'R' => Command::Redo
    }
}

/// The possible meanings of a user-typed command
#[derive(Debug, Clone, Eq, PartialEq)]
enum Action {
    /// The user typed a command that isn't defined, but the command box should still be cleared
    Undefined,
    /// Quit Sapling
    Quit,
    /// Replace the selected node with a node represented by some [`char`]
    Replace(char),
    /// Insert a new node (given by some [`char`]) as the first child of the selected node
    InsertChild(char),
    /// Move the node in a given direction
    MoveCursor(Direction),
    /// Undo the last change
    Undo,
    /// Redo a change
    Redo,
}

/// Attempt to convert a command as a `&`[`str`] into an [`Action`].
/// This parses the string from the start, and returns when it finds a valid command.
///
/// Therefore, `"q489flshb"` will be treated like `"q"`, and will return `Some(Action::Quit)` even
/// though `"q489flshb"` is not technically valid.
/// This function is run every time the user types a command character, and so the user would not
/// be able to input `"q489flshb"` to this function because doing so would require them to first
/// input every possible prefix of `"q489flshb"`, including `"q"`.
///
/// This returns:
/// - [`None`] if the command is incomplete.
/// - [`Action::Undefined`] if the command is not defined (like the command "X").
/// - The corresponding [`Action`], otherwise.
fn parse_command(keymap: &KeyMap, command: &str) -> Option<Action> {
    let mut command_char_iter = command.chars();

    // Consume the first char of the command
    if let Some(c) = command_char_iter.next() {
        match keymap.get(&c) {
            // "q" quits Sapling
            Some(Command::Quit) => {
                return Some(Action::Quit);
            }
            Some(Command::InsertChild) => {
                // Consume the second char of the iterator
                if let Some(insert_char) = command_char_iter.next() {
                    return Some(Action::InsertChild(insert_char));
                }
            }
            Some(Command::Replace) => {
                // Consume the second char of the iterator
                if let Some(replace_char) = command_char_iter.next() {
                    return Some(Action::Replace(replace_char));
                }
            }
            Some(Command::MoveCursor(direction)) => {
                return Some(Action::MoveCursor(*direction));
            }
            Some(Command::Undo) => {
                return Some(Action::Undo);
            }
            Some(Command::Redo) => {
                return Some(Action::Redo);
            }
            None => {
                return Some(Action::Undefined);
            }
        }
    }

    None
}

/// A struct to hold the top-level components of the editor.
pub struct Editor<'arena, Node: Ast<'arena>, E: EditableTree<'arena, Node> + 'arena> {
    /// The [`EditableTree`] that the `Editor` is editing
    tree: &'arena mut E,
    /// The log as a [`Vec`] of logged messages
    log: Vec<(LogLevel, String)>,
    /// The style that the tree is being printed to the screen
    format_style: Node::FormatStyle,
    /// The `tuikit` terminal that the `Editor` is rendering to
    term: Term,
    /// The current contents of the command buffer
    command: String,
    /// The configured key map
    keymap: KeyMap,
}

impl<'arena, Node: Ast<'arena> + 'arena, E: EditableTree<'arena, Node> + 'arena>
    Editor<'arena, Node, E>
{
    /// Create a new [`Editor`] with a given tree
    pub fn new(
        tree: &'arena mut E,
        format_style: Node::FormatStyle,
        keymap: KeyMap,
    ) -> Editor<'arena, Node, E> {
        let term = Term::new().unwrap();
        Editor {
            tree,
            log: Vec::new(),
            term,
            format_style,
            command: String::new(),
            keymap,
        }
    }

    /// Log a message to whatever console is appropriate
    fn log(&mut self, level: LogLevel, message: String) {
        self.log.push((level, message));
    }

    /* ===== COMMAND FUNCTIONS ===== */

    /// Replace the node under the cursor with the node represented by a given [`char`]
    fn replace_cursor(&mut self, c: char) {
        if self.tree.cursor().is_replace_char(c) {
            // We know that `c` corresponds to a valid node, so we can unwrap
            let new_node = self.tree.cursor().from_char(c).unwrap();
            self.log(
                LogLevel::Debug,
                format!("Replacing with '{}'/{:?}", c, new_node),
            );
            self.tree.replace_cursor(new_node);
        } else {
            self.log(
                LogLevel::Warning,
                format!("Cannot replace node with '{}'", c),
            );
        }
    }

    /// Move the cursor
    fn move_cursor(&mut self, direction: Direction) {
        if let Some(error_message) = self.tree.move_cursor(direction) {
            self.log.push((LogLevel::Warning, error_message));
        }
    }

    /// Insert new child as the first child of the selected node
    fn insert_child(&mut self, c: char) {
        if self.tree.cursor().is_insert_char(c) {
            self.log(LogLevel::Debug, format!("Inserting with '{}'", c));
        } else {
            self.log(
                LogLevel::Warning,
                format!("Cannot replace node with '{}'", c),
            );
        }
    }

    /// Undo the latest change
    fn undo(&mut self) {
        if self.tree.undo() {
            self.log(LogLevel::Debug, "Undo successful".to_string());
        } else {
            self.log(LogLevel::Info, "No changes to undo".to_string());
        }
    }

    /// Move one change forward in the history
    fn redo(&mut self) {
        if self.tree.redo() {
            self.log(LogLevel::Debug, "Redo successful".to_string());
        } else {
            self.log(LogLevel::Info, "No changes to redo".to_string());
        }
    }

    /// Render the tree to the screen
    fn render_tree(&self, row: usize, col: usize) {
        // Mutable variables to track where the terminal cursor should go
        let mut row = row;
        let mut col = col;
        let mut indentation_amount = 0;

        let cols = [
            Color::MAGENTA,
            Color::RED,
            Color::YELLOW,
            Color::GREEN,
            Color::CYAN,
            Color::BLUE,
            Color::WHITE,
            Color::LIGHT_RED,
            Color::LIGHT_BLUE,
            Color::LIGHT_CYAN,
            Color::LIGHT_GREEN,
            Color::LIGHT_YELLOW,
            Color::LIGHT_MAGENTA,
            Color::LIGHT_WHITE,
        ];

        /// A cheeky macro to print a string to the terminal
        macro_rules! term_print {
            ($string: expr) => {{
                let string = $string;
                // Print the string
                self.term.print(row, col, string).unwrap();
                // Move the cursor to the end of the string
                let size = size::Size::from(string);
                if size.lines() == 0 {
                    col += size.last_line_length();
                } else {
                    row += size.lines();
                    col = size.last_line_length();
                }
            }};
            ($string: expr, $attr: expr) => {{
                let string = $string;
                // Print the string
                self.term.print_with_attr(row, col, string, $attr).unwrap();
                // Move the cursor to the end of the string
                let size = size::Size::from(string);
                if size.lines() == 0 {
                    col += size.last_line_length();
                } else {
                    row += size.lines();
                    col = size.last_line_length();
                }
            }};
        };

        for (node, tok) in self.tree.root().display_tokens(&self.format_style) {
            match tok {
                DisplayToken::Text(s) => {
                    // Hash the ref to decide on the colour
                    let col = {
                        let mut hasher = DefaultHasher::new();
                        node.hash(&mut hasher);
                        let hash = hasher.finish();
                        cols[hash as usize % cols.len()]
                    };
                    // Generate the display attributes depending on if the node is selected
                    let attr = if std::ptr::eq(node, self.tree.cursor()) {
                        Attr::default().fg(Color::BLACK).bg(col)
                    } else {
                        Attr::default().fg(col)
                    };
                    // Print the token
                    term_print!(s.as_str(), attr);
                }
                DisplayToken::Whitespace(n) => {
                    col += n;
                }
                DisplayToken::Newline => {
                    row += 1;
                    col = indentation_amount;
                }
                DisplayToken::Indent => {
                    indentation_amount += 4;
                }
                DisplayToken::Dedent => {
                    indentation_amount -= 4;
                }
            }
        }
    }

    /* ===== MAIN FUNCTIONS ===== */

    /// Update the terminal UI display
    fn update_display(&self) {
        // Put the terminal size into some convenient variables
        let (width, height) = self.term.term_size().unwrap();

        // Clear the terminal
        self.term.clear().unwrap();

        /* RENDER MAIN TEXT VIEW */
        self.render_tree(0, 0);

        /* RENDER LOG SECTION */
        for (i, (level, message)) in self.log.iter().enumerate() {
            self.term
                .print_with_attr(i, width / 2, message, Attr::default().fg(level.to_color()))
                .unwrap();
        }

        /* RENDER BOTTOM BAR */
        self.term
            .print(height - 1, 0, "Press 'q' to exit.")
            .unwrap();
        self.term
            .print(
                height - 1,
                width - 5 - self.command.chars().count(),
                &self.command,
            )
            .unwrap();

        // Update the terminal screen
        self.term.present().unwrap();
    }

    fn mainloop(&mut self) {
        // Sit in the infinte mainloop
        while let Ok(event) = self.term.poll_event() {
            /* RESPOND TO THE USER'S INPUT */
            if let Event::Key(key) = event {
                match key {
                    Key::Char(c) => {
                        // Add the new keypress to the command
                        self.command.push(c);
                        // Attempt to parse the command, and take action if the command is
                        // complete
                        if let Some(action) = parse_command(&self.keymap, &self.command) {
                            // Respond to the action
                            match action {
                                Action::Undefined => {
                                    self.log(
                                        LogLevel::Warning,
                                        format!("'{}' not a command.", self.command),
                                    );
                                }
                                Action::Quit => {
                                    // Break the mainloop to quit
                                    break;
                                }
                                Action::MoveCursor(direction) => {
                                    self.move_cursor(direction);
                                }
                                Action::Replace(c) => {
                                    self.replace_cursor(c);
                                }
                                Action::InsertChild(c) => {
                                    self.insert_child(c);
                                }
                                Action::Undo => {
                                    self.undo();
                                }
                                Action::Redo => {
                                    self.redo();
                                }
                            }
                            // Clear the command box
                            self.command.clear();
                        }
                    }
                    Key::ESC => {
                        self.command.clear();
                    }
                    _ => {}
                }
            }

            // Update the screen after every input (if this becomes a bottleneck then we can
            // optimise the number of calls to `update_display` but for now it's not worth the
            // added complexity)
            self.update_display();
        }
    }

    /// Start the editor and enter the mainloop
    pub fn run(mut self) {
        // Log the startup of the code
        self.log(LogLevel::Info, "Starting Up...".to_string());
        // Start the mainloop
        self.mainloop();
        // Show the cursor before closing so that the cursor isn't permanently disabled
        // (see issue https://github.com/lotabout/tuikit/issues/28)
        self.term.show_cursor(true).unwrap();
        self.term.present().unwrap();
        // Log that the editor is closing
        self.log(LogLevel::Info, "Closing...".to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_command, Action};
    use crate::editable_tree::Direction;

    #[test]
    fn parse_command_complete() {
        let keymap = super::default_keymap();
        for (command, expected_effect) in &[
            ("q", Action::Quit),
            ("x", Action::Undefined),
            ("pajlbsi", Action::MoveCursor(Direction::Up)),
            ("Pxx", Action::Undefined),
            ("Qsx", Action::Undefined),
            ("ra", Action::Replace('a')),
            ("rg", Action::Replace('g')),
            ("iX", Action::InsertChild('X')),
            ("iP", Action::InsertChild('P')),
        ] {
            assert_eq!(
                parse_command(&keymap, *command),
                Some(expected_effect.clone())
            );
        }
    }

    #[test]
    fn parse_command_incomplete() {
        let keymap = super::default_keymap();
        for command in &["", "r", "i"] {
            assert_eq!(parse_command(&keymap, *command), None);
        }
    }
}
